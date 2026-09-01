'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const WORKER = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
const CANDIDATE = {
  schema: 'saccade.extension-candidate/1', id: 'a'.repeat(64), version: '0.4.1',
};

function eventTarget() {
  return { addListener() {} };
}

function response(body) {
  return { ok: true, json: async () => body };
}

function runWorker({
  authorizedTab = false, closeFirstSocket = false, hangCollectorKinds = [],
} = {}) {
  const calls = [];
  let connects = 0;
  let sockets = 0;
  const nativeSetTimeout = setTimeout;
  const fastSetTimeout = (callback, delay, ...args) => {
    return nativeSetTimeout(callback, Math.min(Number(delay) || 0, 5), ...args);
  };

  class MockWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;

    constructor() {
      this.readyState = MockWebSocket.CONNECTING;
      sockets += 1;
      const socketNumber = sockets;
      queueMicrotask(() => {
        this.readyState = MockWebSocket.OPEN;
        this.onopen?.();
        if (closeFirstSocket && socketNumber === 1) queueMicrotask(() => {
          this.readyState = MockWebSocket.CLOSED;
          this.onclose?.();
        });
      });
    }

    send(value) {
      const message = JSON.parse(value);
      if (message.kind === 'heartbeat') queueMicrotask(() => this.onmessage?.({
        data: JSON.stringify({ kind: 'heartbeat.ack', broker_epoch: 'broker-epoch' }),
      }));
    }

    close() {
      if (this.readyState === MockWebSocket.CLOSED) return;
      this.readyState = MockWebSocket.CLOSED;
      queueMicrotask(() => this.onclose?.());
    }
  }

  const chrome = {
    alarms: { create() {}, onAlarm: eventTarget() },
    runtime: {
      getURL: (value) => `chrome-extension://saccade/${value}`,
      reload() {}, onConnect: eventTarget(), onMessage: eventTarget(),
      onStartup: eventTarget(), onInstalled: eventTarget(),
    },
    storage: {
      local: {
        async get(key) {
          if (key === 'saccade.browser_instance_id') {
            return { 'saccade.browser_instance_id': 'browser-test' };
          }
          if (key === 'saccade.tab_acl' && authorizedTab) {
            return { 'saccade.tab_acl': { agent: [], shared: [7], claimed: [] } };
          }
          return {};
        },
        async set() {}, async remove() {},
      },
      session: {
        async get() {
          return {
            'saccade.browser_session_initialized': true,
            'saccade.connection_session_id': 'browser-session-test',
          };
        },
        async set() {},
      },
    },
    tabs: {
      async get(tabId) { return { id: tabId, url: 'https://example.test/', active: true, windowId: 1 }; },
      async sendMessage(_tabId, message) {
        calls.push(`collector:${message.kind}`);
        if ((message.kind === 'collector.ping' && authorizedTab)
            || hangCollectorKinds.includes(message.kind)) return new Promise(() => {});
        return { ok: true };
      },
      onCreated: eventTarget(), onRemoved: eventTarget(), onUpdated: eventTarget(),
    },
    windows: { WINDOW_ID_NONE: -1, onRemoved: eventTarget() },
  };

  const context = {
    AbortSignal, URL, WebSocket: MockWebSocket, chrome,
    clearInterval() {}, clearTimeout, console: { error() {} },
    importScripts() {}, navigator: { userAgent: 'Chrome' }, queueMicrotask,
    setInterval: () => 1, setTimeout: fastSetTimeout,
    SaccadeCandidate: CANDIDATE,
    SaccadeConsent: {
      isSupportedUrl: (value) => String(value || '').startsWith('https://'),
      normalizeOrigin: (value) => new URL(value).origin,
    },
    SaccadeProtocol: { randomToken: (prefix) => `${prefix}-test` },
    fetch: async (url) => {
      if (String(url).startsWith('chrome-extension://')) return response(CANDIDATE);
      if (String(url).endsWith('/v1/extension/connect')) {
        connects += 1;
        calls.push(`connect:${connects}`);
        return response({
          connection_id: `connection-${connects}`,
          broker_epoch: 'broker-epoch',
          require_full_truth: true,
        });
      }
      if (String(url).endsWith('/v1/extension/commands')) {
        calls.push(`commands:${connects}`);
        return new Promise(() => {});
      }
      if (String(url).endsWith('/v1/extension/events')) return response({ accepted: true });
      throw new Error(`unexpected fetch: ${url}`);
    },
  };
  context.globalThis = context;
  vm.runInNewContext(WORKER, context, { filename: 'service_worker.js' });
  return { calls, connects: () => connects, context };
}

async function waitFor(predicate, timeoutMs = 200) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  assert.fail('condition did not become true before the test deadline');
}

test('a stuck authorized-tab recovery cannot delay the first command poll', async () => {
  const runtime = runWorker({ authorizedTab: true });
  await waitFor(() => runtime.calls.includes('commands:1'));
  assert.ok(runtime.calls.indexOf('commands:1') < runtime.calls.indexOf('collector:collector.ping'));
});

test('a keepalive close during connectPromise cannot lose the reconnect request', async () => {
  const runtime = runWorker({ closeFirstSocket: true });
  await waitFor(() => runtime.connects() >= 2);
  assert.deepEqual(runtime.calls.filter((value) => value.startsWith('connect:')).slice(0, 2), [
    'connect:1', 'connect:2',
  ]);
});

test('a missing post-dispatch Collector response is outcome_unknown and never retry-safe', async () => {
  const runtime = runWorker({ hangCollectorKinds: ['collector.soft_action'] });
  await assert.rejects(
    runtime.context.collectorCommand(7, { kind: 'collector.soft_action' }, 1),
    (error) => error.saccadeCode === 'OUTCOME_UNKNOWN'
      && error.saccadeOutcome === 'outcome_unknown'
      && error.saccadeRetrySafe === false,
  );
  await assert.rejects(
    runtime.context.collectorCommand(
      7, { kind: 'collector.soft_action' }, 1, { sideEffect: false },
    ),
    (error) => error.saccadeCode === 'deadline_exceeded'
      && error.saccadeRetrySafe === true,
  );
});
