'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { once } = require('node:events');
const WebSocket = require('ws');

const {
  BrokerState, EXTENSION_POLL_HEARTBEAT_MS, createBrokerServer, extensionOrigin,
} = require('../src/broker');

function observation(tabId = '7', revision = 1) {
  return {
    schema: 'saccade.observation/1', browser_instance_id: 'browser-1',
    tab_id: tabId, document_id: 'document-1', revision, viewport_revision: revision,
    objects: [{
      object_id: 'object-1', role: 'button', name: 'Continue',
      affordances: ['click'], action_token: 'token-1',
    }],
    changes: [], frames: [], limitations: [],
  };
}

async function deliver(broker, connectionId, promise, result) {
  const [command] = await broker.pollCommands(connectionId, 10);
  broker.acceptExtensionEvents(connectionId, [{
    kind: 'response', command_id: command.command_id, result,
  }]);
  if (command.kind === 'tabs.open' && result.tab_id) {
    broker.acceptExtensionEvents(connectionId, [{ kind: 'observation', payload: observation(result.tab_id) }]);
  }
  return promise;
}

test('tabs.open atomically leases one tab to one Agent session', async () => {
  const broker = new BrokerState();
  const first = broker.createSession().agent_session_id;
  const second = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const opened = broker.rpc(first, 'tabs.open', { url: 'https://example.test' }, 1000);
  const result = await deliver(broker, connection.connection_id, opened, { tab_id: '7', opened: true });
  assert.equal(result.tab_id, '7');
  assert.equal(result.agent_session_id, first);
  assert.deepEqual(broker.listTabs(first).map((tab) => tab.tab_id), ['7']);
  assert.deepEqual(broker.listTabs(second), []);
  assert.throws(() => broker.requireLease('7', second), /another Agent/);
});

test('tabs.open rejects missing or mixed route forms before dispatch', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  await assert.rejects(broker.rpc(session, 'tabs.open', {}, 50), (error) => error.code === 'INVALID_REQUEST');
  await assert.rejects(broker.rpc(session, 'tabs.open', {
    url: 'https://example.test', claim: 'shared', tab_id: '7',
  }, 50), (error) => error.code === 'INVALID_REQUEST');
  assert.equal(broker.commands.size, 0);
});

test('tabs.open requires and obeys exact browser routing when multiple Extensions are online', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const chrome = broker.connectExtension({ browser_instance_id: 'browser-chrome' });
  const edge = broker.connectExtension({ browser_instance_id: 'browser-edge' });
  await assert.rejects(broker.rpc(session, 'tabs.open', {
    url: 'https://example.test',
  }, 50), (error) => error.code === 'AMBIGUOUS_BROWSER'
    && error.candidates.length === 2);

  const pending = broker.rpc(session, 'tabs.open', {
    url: 'https://example.test', browser_instance_id: 'browser-edge',
  }, 1000);
  assert.deepEqual(await broker.pollCommands(chrome.connection_id, 5), []);
  const result = await deliver(broker, edge.connection_id, pending, {
    tab_id: '17', opened: true, browser_instance_id: 'browser-edge',
  });
  assert.equal(result.tab_id, '17');
  assert.equal(broker.leases.get('17').browser_instance_id, 'browser-edge');
});

test('a user-shared tab is explicitly assigned to only one online Agent', async () => {
  const broker = new BrokerState();
  const first = broker.createSession().agent_session_id;
  const second = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(first, 'tabs.open', { claim: 'shared', tab_id: '9' }, 1000);
  const result = await deliver(broker, connection.connection_id, pending, {
    tab_id: '9', opened: false, provenance: 'user_shared',
  });
  assert.equal(result.lease, 'active');
  assert.equal(broker.leases.get('9').ownership, 'user_shared');
  await assert.rejects(broker.rpc(second, 'tabs.open', {
    claim: 'shared', tab_id: '9',
  }, 50), (error) => error.code === 'TAB_ALREADY_LEASED');
});

test('full and delta reads are explicit and exact-tab only', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  const full = await broker.readTruth(session, { tab_id: '7', mode: 'full' }, Date.now() + 50);
  assert.equal(full.mode, 'full');
  assert.equal(full.tab_id, '7');
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 2, objects: [{
      object_id: 'object-2', role: 'button', name: 'Next',
      affordances: ['click'], action_token: 'token-2',
    }], authorities: [],
    changes: [
      { kind: 'disappeared', object_id: 'object-1', object_revision: 1 },
      { kind: 'appeared', object_id: 'object-2', object_revision: 1 },
    ],
  });
  const delta = await broker.readTruth(session, { tab_id: '7', mode: 'delta', after_revision: 1 }, Date.now() + 50);
  assert.equal(delta.mode, 'delta');
  assert.equal(delta.changes.length, 2);
  assert.deepEqual(delta.objects.map((object) => object.object_id), ['object-2']);
  const reset = await broker.readTruth(session, { tab_id: '7', mode: 'delta', after_revision: 0 }, Date.now() + 50);
  assert.equal(reset.reset_required, true);
  assert.equal(reset.get, undefined);
});

test('semantic truth reads keep authorities scoped to the working set', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', {
    ...observation(),
    authorities: [
      { object_id: 'object-1', action_token: 'token-1' },
      { object_id: 'object-2', action_token: 'token-2' },
    ],
    objects: [
      {
        object_id: 'object-1', role: 'button', name: 'Alpha', text: 'Alpha',
        affordances: ['click'], action_token: 'token-1',
      },
      {
        object_id: 'object-2', role: 'button', name: 'Beta', text: 'Beta',
        affordances: ['click'], action_token: 'token-2',
      },
    ],
  });
  const result = await broker.readTruth(session, {
    tab_id: '7', mode: 'full',
    query: { text: 'Alpha', max_objects: 32 },
    min_objects: 1,
  }, Date.now() + 50);
  assert.deepEqual(result.objects.map((object) => object.object_id), ['object-1']);
  assert.deepEqual(result.authorities, [{ object_id: 'object-1', action_token: 'token-1' }]);
});

test('first full read automatically compacts a large complete catalog', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = Array.from({ length: 65 }, (_, index) => ({
    object_id: `object-${index}`, object_revision: 1, role: 'button',
    name: `Button ${index}`, text: 'large page detail', affordances: ['click'],
    action_token: `token-${index}`,
  }));
  broker.acceptTruth('observation', full);
  const result = await broker.readTruth(session, { tab_id: '7', mode: 'full' }, Date.now() + 50);
  assert.equal(result.catalog, 'complete_compact');
  assert.equal(result.object_count, 65);
  assert.equal(result.objects.length, 65);
  assert.equal(result.objects[0].text, undefined);
});

test('delta read waits locally for a pushed revision instead of polling', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  const pending = broker.readTruth(session, {
    tab_id: '7', mode: 'delta', after_revision: 1,
  }, Date.now() + 200);
  setTimeout(() => broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 2, objects: [], authorities: [], changes: [],
  }), 5);
  const result = await pending;
  assert.equal(result.revision, 2);
  assert.equal(result.timed_out, undefined);
});

test('semantic read waits for the requested working set and bounds related authority', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.authorities = [{ object_id: 'object-1', action_token: 'token-1' }];
  broker.acceptTruth('observation', full);
  const pending = broker.readTruth(session, {
    tab_id: '7', mode: 'full', query: { text: 'Ready' }, min_objects: 1,
  }, Date.now() + 200);
  setTimeout(() => broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 1,
    objects: [{ object_id: 'ready', role: 'status', name: 'Ready', affordances: [] }],
    authorities: [{ object_id: 'object-1', action_token: 'token-1' }],
    changes: [{ kind: 'appeared', object_id: 'ready', object_revision: 1 }],
  }), 5);
  const result = await pending;
  assert.deepEqual(result.objects.map((object) => object.object_id), ['ready']);
  assert.deepEqual(result.authorities, []);
  assert.equal(result.match_count, 1);
});

test('Agent disconnect orphans leases without transfer or close', () => {
  const broker = new BrokerState();
  const first = broker.createSession().agent_session_id;
  const second = broker.createSession().agent_session_id;
  broker.leaseTab('7', first);
  assert.equal(broker.closeSession(first).orphaned_tabs, 1);
  assert.throws(() => broker.leaseTab('7', second), /writer/);
  assert.equal(broker.leases.get('7').state, 'orphaned');
});

test('session IDs alone cannot authorize loopback RPC access', () => {
  const broker = new BrokerState();
  const session = broker.createSession();
  assert.throws(() => broker.authorizeSession(session.agent_session_id, 'resume_wrong-proof'), (error) => error.code === 'SESSION_AUTH_FAILED');
  assert.equal(
    broker.authorizeSession(session.agent_session_id, session.resume_token).agent_session_id,
    session.agent_session_id,
  );
});

test('a delivered action is never replayed after Extension reconnect', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const first = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'act', { tab_id: '7' }, 1000);
  const [command] = await broker.pollCommands(first.connection_id, 10);
  assert.equal(command.kind, 'act');
  broker.disconnectExtension(first.connection_id, 'power_loss');
  await assert.rejects(pending, (error) => error.code === 'OUTCOME_UNKNOWN' && error.retry_safe === false);
  const second = broker.connectExtension({ browser_instance_id: 'browser-1' });
  assert.deepEqual(await broker.pollCommands(second.connection_id, 5), []);
});

test('queued work belongs to the browser and is claimed only on Extension delivery', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const first = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'tabs.open', { url: 'https://example.test' }, 1000);
  const queued = [...broker.commands.values()].at(-1);
  assert.equal(queued.browser_instance_id, 'browser-1');
  assert.equal(queued.connection_id, null);

  const second = broker.connectExtension({ browser_instance_id: 'browser-1' });
  assert.equal(broker.connections.get(first.connection_id).state, 'offline');
  const [command] = await broker.pollCommands(second.connection_id, 10);
  assert.equal(command.kind, 'tabs.open');
  assert.equal(command.payload.url, 'https://example.test');
  assert.equal(queued.connection_id, second.connection_id);

  broker.acceptExtensionEvents(second.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { tab_id: '7' },
  }]);
  assert.equal((await pending).tab_id, '7');
  assert.equal(broker.occurrences.at(-1).occurrence, 'acknowledged');
});

test('Extension loss rejects queued work immediately when no reconnect is pending', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1000);
  broker.disconnectExtension(connection.connection_id, 'power_loss');
  await assert.rejects(pending, (error) => (
    error.code === 'EXTENSION_OFFLINE' && error.retry_safe === true
  ));
});

test('a renewed consumer cannot claim another browser instance queue', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.connectExtension({ browser_instance_id: 'browser-1' });
  const other = broker.connectExtension({ browser_instance_id: 'browser-2' });
  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1000, {
    browserInstanceId: 'browser-1',
  });

  assert.deepEqual(await broker.pollCommands(other.connection_id, 5), []);
  const renewed = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const [command] = await broker.pollCommands(renewed.connection_id, 10);
  assert.equal(command.kind, 'tabs.open');
  broker.acceptExtensionEvents(renewed.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { tab_id: '8' },
  }]);
  assert.equal((await pending).tab_id, '8');
});

test('replacement diagnostics identify browser-family consumer contention', () => {
  const broker = new BrokerState();
  broker.connectExtension({
    browser_instance_id: 'browser-1', browser_family: 'chrome',
    browser_session_id: 'session-1', worker_instance_id: 'worker-1',
  });
  broker.connectExtension({
    browser_instance_id: 'browser-1', browser_family: 'edge',
    browser_session_id: 'session-1', worker_instance_id: 'worker-2',
  });
  const replacement = broker.doctor().recent_failures.at(-1);
  assert.equal(replacement.code, 'replaced_connection');
  assert.equal(replacement.browser_family, 'chrome');
  assert.equal(replacement.replacement_browser_family, 'edge');
  assert.equal(replacement.same_browser_session, true);
  assert.equal(replacement.same_worker_instance, false);
  assert.equal(replacement.poll_count, 0);
  assert.equal(replacement.connection_age_ms, 0);
});

test('capabilities prove the attached browser family and exact Extension candidate', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const candidate = {
    schema: 'saccade.extension-candidate/1',
    id: 'a'.repeat(64),
    version: '0.4.0',
  };
  broker.connectExtension({
    browser_instance_id: 'browser-1', browser_family: 'chrome',
    extension_candidate: candidate,
  });
  broker.leaseTab('7', session, { browser_instance_id: 'browser-1', ownership: 'agent' });

  const capabilities = await broker.rpc(session, 'system.capabilities');
  assert.equal(capabilities.schema, 'saccade.capabilities/8');
  assert.equal(capabilities.browser_family, 'chrome');
  assert.deepEqual(capabilities.extension_candidate, candidate);
  assert.deepEqual(capabilities.connected_extensions, [{
    browser_instance_id: 'browser-1', browser_family: 'chrome', extension_candidate: candidate,
  }]);
  assert.equal(capabilities.leased_tabs[0].browser_family, 'chrome');
  assert.deepEqual(capabilities.leased_tabs[0].extension_candidate, candidate);
});

test('Extension handshake rejects unbounded or unrecognized candidate metadata', () => {
  const broker = new BrokerState();
  assert.throws(() => broker.connectExtension({
    browser_instance_id: 'browser-1', browser_family: 'safari',
    extension_candidate: { schema: 'saccade.extension-candidate/1', id: 'a'.repeat(64), version: '0.4.0' },
  }), /browser_family is invalid/);
  assert.throws(() => broker.connectExtension({
    browser_instance_id: 'browser-1', browser_family: 'edge',
    extension_candidate: { schema: 'saccade.extension-candidate/1', id: 'not-a-digest', version: '0.4.0' },
  }), /extension_candidate is invalid/);
});

test('an expired long-poll waiter cannot swallow the next command', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  assert.deepEqual(await broker.pollCommands(connection.connection_id, 2), []);
  assert.equal(broker.connections.get(connection.connection_id).waiters.length, 0);

  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1000);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(command.kind, 'tabs.open');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { tab_id: '7' },
  }]);
  assert.equal((await pending).tab_id, '7');
  assert.ok(EXTENSION_POLL_HEARTBEAT_MS < 4_000);
});

test('Broker restart resumes the same proven session and lease without persisting Truth', (context) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-broker-state-'));
  context.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const statePath = path.join(directory, 'broker-state.json');
  const firstBroker = new BrokerState({ statePath });
  const firstSession = firstBroker.createSession();
  firstBroker.leaseTab('7', firstSession.agent_session_id, {
    browser_instance_id: 'browser-1', ownership: 'agent',
  });
  const full = observation();
  full.objects[0].name = 'page-secret-that-must-not-persist';
  firstBroker.acceptTruth('observation', full);

  const stored = fs.readFileSync(statePath, 'utf8');
  assert.doesNotMatch(stored, /page-secret-that-must-not-persist|action_token|token-1/);
  assert.doesNotMatch(stored, new RegExp(firstSession.resume_token));

  const restarted = new BrokerState({ statePath });
  assert.equal(restarted.doctor().recoverable_sessions, 1);
  assert.equal(restarted.doctor().recoverable_leases, 1);
  assert.equal(restarted.truth.size, 0);
  assert.throws(() => restarted.createSession({ resume_token: 'resume_invalid-proof' }), (error) => error.code === 'RESUME_DENIED');

  const resumed = restarted.createSession({ resume_token: firstSession.resume_token });
  assert.equal(resumed.agent_session_id, firstSession.agent_session_id);
  assert.equal(resumed.resumed, true);
  assert.equal(resumed.resumed_tabs, 1);
  assert.deepEqual(restarted.listTabs(resumed.agent_session_id).map((tab) => ({
    tab_id: tab.tab_id, readiness: tab.readiness,
  })), [{ tab_id: '7', readiness: 'awaiting_truth' }]);
  restarted.acceptTruth('observation', observation());
  assert.equal(restarted.readTruthNow(resumed.agent_session_id, { tab_id: '7', mode: 'full' }).revision, 1);

  const secondRestart = new BrokerState({ statePath });
  assert.throws(() => secondRestart.createSession({ resume_token: firstSession.resume_token }), (error) => error.code === 'RESUME_DENIED');
  assert.equal(secondRestart.createSession({ resume_token: resumed.resume_token }).agent_session_id, firstSession.agent_session_id);
});

test('Broker restart records dispatched work as outcome_unknown and never stores its payload', async (context) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-broker-occurrence-'));
  context.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const statePath = path.join(directory, 'broker-state.json');
  const firstBroker = new BrokerState({ statePath });
  const session = firstBroker.createSession();
  const connection = firstBroker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = firstBroker.enqueueCommand(session.agent_session_id, 'act', {
    tab_id: '7', text: 'side-effect-secret',
  }, 1000);
  pending.catch(() => null);
  const [delivered] = await firstBroker.pollCommands(connection.connection_id, 10);
  assert.equal(delivered.kind, 'act');
  assert.doesNotMatch(fs.readFileSync(statePath, 'utf8'), /side-effect-secret/);

  const restarted = new BrokerState({ statePath });
  assert.equal(restarted.commands.size, 0);
  assert.equal(restarted.doctor().outcome_unknown_occurrences, 1);
  const replacement = restarted.connectExtension({ browser_instance_id: 'browser-1' });
  assert.deepEqual(await restarted.pollCommands(replacement.connection_id, 5), []);

  firstBroker.disconnectExtension(connection.connection_id, 'test_end');
  await assert.rejects(pending, (error) => error.code === 'OUTCOME_UNKNOWN');
});

test('state write failure never acknowledges a delivered command or leaves a new lease active', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'act', { tab_id: '7' }, 1000);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.persistState = () => { throw Object.assign(new Error('disk full'), { code: 'STATE_PERSIST_FAILED' }); };
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { accepted: true },
  }]);
  await assert.rejects(pending, (error) => (
    error.code === 'OUTCOME_UNKNOWN' && error.retry_safe === false && error.stage === 'broker_state'
  ));
  assert.throws(() => broker.leaseTab('8', session), (error) => error.code === 'STATE_PERSIST_FAILED');
  assert.equal(broker.leases.has('8'), false);
});

test('clean Agent close revokes resume proof and preserves an orphaned lease', (context) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-broker-close-'));
  context.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const statePath = path.join(directory, 'broker-state.json');
  const broker = new BrokerState({ statePath });
  const session = broker.createSession();
  broker.leaseTab('7', session.agent_session_id);
  broker.closeSession(session.agent_session_id);

  const restarted = new BrokerState({ statePath });
  assert.equal(restarted.leases.get('7').state, 'orphaned');
  assert.throws(() => restarted.createSession({ resume_token: session.resume_token }), (error) => error.code === 'RESUME_DENIED');
  const other = restarted.createSession().agent_session_id;
  assert.throws(() => restarted.leaseTab('7', other), /writer/);
});

test('cancellation removes queued commands but never claims to cancel delivered work', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const queued = broker.enqueueCommand(session, 'tabs.open', {}, 1000, { clientRequestId: 11 });
  assert.deepEqual(broker.cancelRequest(session, 11), { cancelled: true, dispatched: false });
  await assert.rejects(queued, (error) => error.code === 'CANCELLED' && error.retry_safe === true);

  const delivered = broker.enqueueCommand(session, 'act', {}, 1000, { clientRequestId: 12 });
  await broker.pollCommands(connection.connection_id, 10);
  assert.deepEqual(broker.cancelRequest(session, 12), {
    cancelled: false, dispatched: true, reconciliation_required: true,
  });
  broker.disconnectExtension(connection.connection_id, 'test_end');
  await assert.rejects(delivered, (error) => error.code === 'OUTCOME_UNKNOWN');
});

test('single-file upload is workspace-bounded, hash-pinned, and absent from the receipt', async (context) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-upload-'));
  context.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const filePath = path.join(directory, 'gameplay.jpg');
  const content = Buffer.from('bounded-image-fixture');
  fs.writeFileSync(filePath, content);
  const sha256 = crypto.createHash('sha256').update(content).digest('hex');

  const broker = new BrokerState({ uploadRoots: [directory] });
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [{
    object_id: 'upload-1', role: 'file_input', name: 'Upload screenshots',
    affordances: ['upload'], action_token: 'upload-token', state: { has_value: 'false' },
  }];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'upload-1', operation: 'upload',
    file_path: filePath, file_sha256: sha256, timeout_ms: 500,
  }, 500, 26);
  const [command] = await broker.pollCommands(connection.connection_id, 20);
  assert.equal(command.kind, 'act');
  assert.equal(command.payload.payload.kind, 'file');
  assert.equal(command.payload.payload.file.name, 'gameplay.jpg');
  assert.equal(command.payload.payload.file.mime_type, 'image/jpeg');
  assert.equal(command.payload.payload.file.size_bytes, content.length);
  assert.equal(Buffer.from(command.payload.payload.file.content_base64, 'base64').toString(), content.toString());

  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1,
      upload_dispatch: 'file_input',
      semantic_postcondition: { code: 'file_selection_observed', verified: true },
    },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1, authorities: [],
      objects: [{
        object_id: 'upload-1', role: 'file_input', name: 'Upload screenshots',
        affordances: ['upload'], action_token: 'upload-token', state: { has_value: 'true' },
      }],
      changes: [{ kind: 'updated', object_id: 'upload-1', object_revision: 2 }],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.equal(receipt.semantic_postcondition.code, 'file_selection_observed');
  assert.equal(receipt.external_execution_required, true);
  assert.deepEqual(receipt.upload, { size_bytes: content.length, mime_type: 'image/jpeg', sha256 });
  assert.doesNotMatch(JSON.stringify(receipt), /gameplay\.jpg|content_base64|saccade-upload-/);
  assert.equal(command.payload.payload.file.content_base64, undefined);
});

test('upload rejects an unapproved path or changed file before dispatch', async (context) => {
  const allowed = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-upload-allowed-'));
  const denied = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-upload-denied-'));
  context.after(() => fs.rmSync(allowed, { recursive: true, force: true }));
  context.after(() => fs.rmSync(denied, { recursive: true, force: true }));
  const deniedPath = path.join(denied, 'private.png');
  fs.writeFileSync(deniedPath, 'not-readable-through-saccade');

  const broker = new BrokerState({ uploadRoots: [allowed] });
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [{
    object_id: 'upload-1', role: 'file_input', affordances: ['upload'], action_token: 'upload-token',
  }];
  broker.acceptTruth('observation', full);
  broker.connectExtension({ browser_instance_id: 'browser-1' });
  const basis = {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'upload-1', operation: 'upload', file_path: deniedPath,
  };
  await assert.rejects(broker.rpc(session, 'act', basis, 100), (error) => (
    error.code === 'UPLOAD_PATH_DENIED' && error.retry_safe === true
  ));

  const allowedPath = path.join(allowed, 'changed.png');
  fs.writeFileSync(allowedPath, 'current');
  await assert.rejects(broker.rpc(session, 'act', {
    ...basis, file_path: allowedPath, file_sha256: '0'.repeat(64),
  }, 100), (error) => error.code === 'UPLOAD_HASH_MISMATCH');
  assert.equal(broker.commands.size, 0);
});

test('form batch preflights every independent object before one dispatch', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [
    { object_id: 'name', role: 'text_field', affordances: ['type'], action_token: 'token-name' },
    { object_id: 'country', role: 'select', affordances: ['select'], action_token: 'token-country' },
    { object_id: 'us', role: 'option', affordances: ['click'], action_token: 'token-us' },
  ];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1, timeout_ms: 200,
    steps: [
      { object_id: 'name', operation: 'type', text: 'secret-not-in-receipt' },
      { object_id: 'country', operation: 'select', option_object_id: 'us' },
    ],
  }, 200, 21);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(command.kind, 'act.batch');
  assert.equal(command.payload.steps[0].browser_instance_id, 'browser-1');
  assert.deepEqual(command.payload.steps.map((step) => step.object_id), ['name', 'country']);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: { accepted: true, steps: [{ accepted: true }, { accepted: true }] },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1, objects: [], authorities: [],
      changes: [
        { kind: 'updated', object_id: 'name', object_revision: 2 },
        { kind: 'updated', object_id: 'country', object_revision: 2 },
        { kind: 'updated', object_id: 'us', object_revision: 2 },
      ],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.deepEqual(receipt.steps.map((step) => step.step_index), [0, 1]);
  assert.deepEqual(receipt.relevant_delta.changed_steps, [0, 1]);
  assert.equal(receipt.relevant_delta.schema, 'saccade.action-delta/1');
  assert.equal(receipt.relevant_delta.base_revision, 1);
  assert.equal(receipt.relevant_delta.objects, undefined);
  assert.doesNotMatch(JSON.stringify(receipt), /document_bounds|viewport_bounds|action_token/);
  assert.doesNotMatch(JSON.stringify(receipt), /secret-not-in-receipt/);
});

test('ordinary action safely rebases across contiguous unrelated Truth changes', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 1,
    objects: [{ object_id: 'status', role: 'status', text: 'Cycle 1', affordances: [] }],
    authorities: [{ object_id: 'object-1', action_token: 'token-1' }],
    changes: [{ kind: 'appeared', object_id: 'status', object_revision: 2 }],
  });
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 2, revision: 3,
    viewport_revision: 1,
    objects: [{ object_id: 'status', role: 'status', text: 'Cycle 2', affordances: [] }],
    authorities: [{ object_id: 'object-1', action_token: 'token-1' }],
    changes: [{ kind: 'updated', object_id: 'status', object_revision: 3 }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'object-1', operation: 'click', timeout_ms: 200,
  }, 200, 31);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(command.kind, 'act');
  assert.equal(command.payload.basis_revision, 3);
  assert.equal(command.payload.object_id, 'object-1');
  assert.equal(command.payload.action_token, 'token-1');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 3,
      semantic_postcondition: { code: 'click_dispatched', verified: true },
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.equal(receipt.rebased_from_revision, 1);
  assert.equal(receipt.dispatch_basis_revision, 3);
  assert.equal(receipt.final_revision, 3);
  assert.equal(receipt.retry_safe, false);
});

test('ordinary action rejects revision drift when its target changed', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 1,
    objects: [{
      object_id: 'object-1', role: 'button', name: 'Continue', state: { enabled: 'false' },
      affordances: ['click'], action_token: 'token-2',
    }],
    authorities: [],
    changes: [{ kind: 'updated', object_id: 'object-1', object_revision: 2 }],
  });
  broker.connectExtension({ browser_instance_id: 'browser-1' });
  await assert.rejects(broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'object-1', operation: 'click',
  }, 50, 32), (error) => error.code === 'STALE_AUTHORITY'
    && error.retry_safe === true && error.current_revision === 2);
  assert.equal(broker.commands.size, 0);
});

test('ordinary action rejects revision drift across a missing history basis', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation('7', 2));
  broker.connectExtension({ browser_instance_id: 'browser-1' });
  await assert.rejects(broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'object-1', operation: 'click',
  }, 50, 33), (error) => error.code === 'STALE_AUTHORITY'
    && error.retry_safe === true && error.current_revision === 2);
  assert.equal(broker.commands.size, 0);
});

test('form batch safely rebases when every addressed identity stayed unchanged', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [
    { object_id: 'name', role: 'text_field', affordances: ['type'], action_token: 'token-name' },
    { object_id: 'newsletter', role: 'checkbox', affordances: ['click'], action_token: 'token-newsletter' },
  ];
  broker.acceptTruth('observation', full);
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 1,
    objects: [{ object_id: 'status', role: 'status', text: 'Ambient update', affordances: [] }],
    authorities: [
      { object_id: 'name', action_token: 'token-name' },
      { object_id: 'newsletter', action_token: 'token-newsletter' },
    ],
    changes: [{ kind: 'appeared', object_id: 'status', object_revision: 2 }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    steps: [
      { object_id: 'name', operation: 'type', text: 'not-returned' },
      { object_id: 'newsletter', operation: 'click' },
    ],
  }, 200, 35);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(command.kind, 'act.batch');
  assert.equal(command.payload.basis_revision, 2);
  assert.deepEqual(command.payload.steps.map((step) => step.basis_revision), [2, 2]);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 2,
      steps: [
        { accepted: true, semantic_postcondition: { verified: true } },
        { accepted: true, semantic_postcondition: { verified: true } },
      ],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.equal(receipt.rebased_from_revision, 1);
  assert.deepEqual(receipt.steps.map((step) => step.verified), [true, true]);
  assert.doesNotMatch(JSON.stringify(receipt), /not-returned/);
});

test('form batch rejects stale rebase when a selected option changed', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [
    { object_id: 'country', role: 'select', affordances: ['select'], action_token: 'token-country' },
    { object_id: 'us', role: 'option', affordances: ['click'], action_token: 'token-us' },
  ];
  broker.acceptTruth('observation', full);
  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 1,
    objects: [{ object_id: 'us', role: 'option', state: { enabled: 'false' }, affordances: [] }],
    authorities: [{ object_id: 'country', action_token: 'token-country' }],
    changes: [{ kind: 'updated', object_id: 'us', object_revision: 2 }],
  });
  broker.connectExtension({ browser_instance_id: 'browser-1' });
  await assert.rejects(broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    steps: [{ object_id: 'country', operation: 'select', option_object_id: 'us' }],
  }, 50, 34), (error) => error.code === 'STALE_AUTHORITY' && error.retry_safe === true);
  assert.equal(broker.commands.size, 0);
});

test('form batch stays outcome_unknown until every accepted step has a relevant Truth change', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [
    { object_id: 'name', role: 'text_field', affordances: ['type'], action_token: 'token-name' },
    { object_id: 'newsletter', role: 'checkbox', affordances: ['click'], action_token: 'token-newsletter' },
  ];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1, timeout_ms: 200,
    steps: [
      { object_id: 'name', operation: 'type', text: 'not-returned' },
      { object_id: 'newsletter', operation: 'click' },
    ],
  }, 200, 29);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: { accepted: true, steps: [{ accepted: true }, { accepted: true }] },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1, objects: [], authorities: [],
      changes: [{ kind: 'updated', object_id: 'name', object_revision: 2 }],
    },
  }]);

  const receipt = await pending;
  assert.equal(receipt.outcome, 'outcome_unknown');
  assert.equal(receipt.occurrence, 'dispatched');
  assert.deepEqual(receipt.semantic_postcondition, {
    code: 'batch_verification_incomplete', stage: undefined, verified: false,
  });
  assert.deepEqual(receipt.steps.map((step) => step.verified), [true, false]);
  assert.equal(receipt.retry_safe, false);
  assert.doesNotMatch(JSON.stringify(receipt), /not-returned/);
});

test('partially dispatched batch is outcome_unknown and never retry-safe', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [
    { object_id: 'first', role: 'text_field', affordances: ['type'], action_token: 'token-first' },
    { object_id: 'second', role: 'text_field', affordances: ['type'], action_token: 'token-second' },
  ];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1, timeout_ms: 200,
    steps: [
      { object_id: 'first', operation: 'type', text: 'not-returned' },
      { object_id: 'second', operation: 'type', text: 'also-not-returned' },
    ],
  }, 200, 24);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: false, partial_dispatch: true,
      failure_code: 'stale_action_token',
      dispatch_document_id: 'document-1', dispatch_basis_revision: 1,
      steps: [{ accepted: true }, { accepted: false, code: 'stale_action_token' }],
    },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1, authorities: [],
      objects: [{ object_id: 'first', role: 'text_field', affordances: ['type'], action_token: 'token-first' }],
      changes: [{ kind: 'updated', object_id: 'first', object_revision: 2 }],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'outcome_unknown');
  assert.equal(receipt.occurrence, 'partially_dispatched');
  assert.equal(receipt.retry_safe, false);
  assert.equal(receipt.semantic_postcondition.code, 'stale_action_token');
  assert.deepEqual(receipt.steps.map((step) => step.accepted), [true, false]);
  assert.doesNotMatch(JSON.stringify(receipt), /not-returned/);
});

test('pre-dispatch batch rejection preserves its value-free failure diagnostics', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [{
    object_id: 'first', role: 'text_field', affordances: ['type'], action_token: 'token-first',
  }];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    steps: [{ object_id: 'first', operation: 'type', text: 'not-returned' }],
  }, 200, 25);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: false, partial_dispatch: false,
      failure_stage: 'prepare', failure_code: 'actionability_timeout_not_topmost',
      retry_safe: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1,
      steps: [{ accepted: false, code: 'actionability_timeout_not_topmost' }],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'rejected');
  assert.deepEqual(receipt.semantic_postcondition, {
    code: 'actionability_timeout_not_topmost', stage: 'prepare', verified: false,
  });
  assert.equal(receipt.retry_safe, true);
  assert.doesNotMatch(JSON.stringify(receipt), /not-returned/);
});

test('form batch rejects submit-like clicks before dispatch', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  await assert.rejects(broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    steps: [{ object_id: 'object-1', operation: 'click' }],
  }, 50), (error) => error.code === 'BATCH_BOUNDARY');
  assert.equal(broker.commands.size, 0);
});

test('act rejects missing or mixed single and batch forms before dispatch', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  const basis = { tab_id: '7', document_id: 'document-1', basis_revision: 1 };
  await assert.rejects(broker.rpc(session, 'act', basis, 50), (error) => error.code === 'INVALID_REQUEST');
  await assert.rejects(broker.rpc(session, 'act', {
    ...basis, object_id: 'object-1', steps: [{ object_id: 'object-1' }],
  }, 50), (error) => error.code === 'INVALID_REQUEST');
  assert.equal(broker.commands.size, 0);
});

test('action verification starts after the Extension dispatch basis', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', observation());
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'object-1', operation: 'click', timeout_ms: 200,
  }, 200, 22);
  const [command] = await broker.pollCommands(connection.connection_id, 10);

  broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
    viewport_revision: 2, objects: [], authorities: [],
    changes: [{ kind: 'updated', object_id: 'other-object', object_revision: 1 }],
  });
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 2,
    },
  }]);
  setTimeout(() => broker.acceptTruth('observation.delta', {
    tab_id: '7', document_id: 'document-1', base_revision: 2, revision: 3,
    viewport_revision: 2, objects: [], authorities: [],
    changes: [{ kind: 'updated', object_id: 'object-1', object_revision: 2 }],
  }), 5);

  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.equal(receipt.dispatch_basis_revision, 2);
  assert.equal(receipt.final_revision, 3);
  assert.equal(receipt.relevant_delta.next_basis_revision, 3);
  assert.deepEqual(receipt.relevant_delta.changes.map((change) => change.object_id), ['object-1']);
});

test('a value-free Extension postcondition verifies typing without exposing editable contents', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects = [{
    object_id: 'editor', role: 'content_editable', affordances: ['type'],
    action_token: 'editor-token', state: { has_value: 'true' },
  }];
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'editor', operation: 'type', text: 'never-return-this', timeout_ms: 200,
  }, 200, 30);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: {
      accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1,
      semantic_postcondition: { code: 'editable_content_observed', verified: true },
    },
  }]);

  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.equal(receipt.occurrence, 'observed');
  assert.deepEqual(receipt.semantic_postcondition, {
    code: 'editable_content_observed', stage: undefined, verified: true,
  });
  assert.equal(receipt.final_revision, 1);
  assert.equal(receipt.relevant_delta, undefined);
  assert.doesNotMatch(JSON.stringify(receipt), /never-return-this/);
});

test('bounded reflex execution stays inside one act request and verifies each occurrence', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const loopClass = 'loop-current';
  broker.acceptTruth('observation', {
    ...observation(),
    objects: [{
      object_id: 'loop-controller', role: 'reflex_target', affordances: [],
      loop_class_token: loopClass, state: { enabled: 'false', reflex_occurrence: '0' },
    }, {
      object_id: 'reflex-1', role: 'reflex_target', affordances: ['click'],
      action_token: 'reflex-token-1', loop_class_token: loopClass,
      state: { enabled: 'true', reflex_occurrence: '0' },
    }, {
      object_id: 'start-1', role: 'button', affordances: ['click'], action_token: 'start-token-1',
    }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'loop-controller', operation: 'click', max_actions: 1,
    start_object_id: 'start-1', timeout_ms: 500,
  }, 500, 27);
  const [startCommand] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(startCommand.kind, 'act');
  assert.equal(startCommand.payload.object_id, 'start-1');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: startCommand.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1,
      objects: [{
        object_id: 'loop-controller', role: 'reflex_target', affordances: [],
        loop_class_token: loopClass, state: { enabled: 'false', reflex_occurrence: '0' },
      }, {
        object_id: 'reflex-1', role: 'reflex_target', affordances: ['click'],
        action_token: 'reflex-token-1', loop_class_token: loopClass,
        state: { enabled: 'true', reflex_occurrence: '0' },
      }],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'loop-controller', object_revision: 2 },
        { kind: 'disappeared', object_id: 'start-1', object_revision: 1 },
      ],
    },
  }]);
  const [command] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(command.kind, 'act');
  assert.equal(command.payload.object_id, 'reflex-1');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 2 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 2, revision: 3,
      viewport_revision: 1,
      objects: [{
        object_id: 'loop-controller', role: 'reflex_target', affordances: [],
        loop_class_token: loopClass, state: { enabled: 'false', reflex_occurrence: '1' },
      }],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'loop-controller', object_revision: 3 },
        { kind: 'disappeared', object_id: 'reflex-1', object_revision: 2 },
      ],
    },
  }]);
  const report = await pending;
  assert.equal(report.schema, 'saccade.reflex-report/1');
  assert.equal(report.actions, 1);
  assert.equal(report.stop_reason, 'max_actions');
  assert.equal(report.semantic_postcondition.verified, true);
  assert.deepEqual(report.receipts.map((receipt) => [receipt.before_occurrence, receipt.after_occurrence]), [['0', '1']]);
});

test('bounded reflex controller safely rebases across unrelated moving-target revisions', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const loopClass = 'loop-moving';
  broker.acceptTruth('observation', {
    ...observation('7', 9),
    objects: [{
      object_id: 'loop-controller', role: 'reflex_target', affordances: [],
      loop_class_token: loopClass, state: { enabled: 'false', reflex_occurrence: '0' },
    }, {
      object_id: 'moving-target', role: 'reflex_target', affordances: ['click'],
      action_token: 'moving-token', loop_class_token: loopClass,
      state: { enabled: 'true', reflex_occurrence: '0' },
    }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'loop-controller', operation: 'click', max_actions: 1,
    timeout_ms: 500,
  }, 500, 28);
  const [command] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(command.payload.object_id, 'moving-target');
  assert.equal(command.payload.basis_revision, 9);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 9 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 9, revision: 10,
      viewport_revision: 10,
      objects: [{
        object_id: 'loop-controller', role: 'reflex_target', affordances: [],
        loop_class_token: loopClass, state: { enabled: 'false', reflex_occurrence: '1' },
      }],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'loop-controller', object_revision: 10 },
        { kind: 'disappeared', object_id: 'moving-target', object_revision: 9 },
      ],
    },
  }]);
  const report = await pending;
  assert.equal(report.actions, 1);
  assert.equal(report.stop_reason, 'max_actions');
  assert.equal(report.semantic_postcondition.verified, true);
});

test('bounded reflex launch follows one explicit same-origin start navigation then resolves a new controller', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', {
    ...observation(),
    frames: [{
      frame_id: 'frame-1', document_id: 'document-1',
      document_url: 'https://game.test/', status: 'observed',
    }],
    objects: [{
      object_id: 'start-link', role: 'link', affordances: ['click'],
      action_token: 'start-token', navigation_target: 'https://game.test/game',
    }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    max_actions: 1, start_object_id: 'start-link', timeout_ms: 500,
  }, 500, 29);
  const [startCommand] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(startCommand.payload.object_id, 'start-link');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: startCommand.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1 },
  }, {
    kind: 'observation', payload: {
      ...observation('7', 1), document_id: 'document-2',
      frames: [{
        frame_id: 'frame-2', document_id: 'document-2',
        document_url: 'https://game.test/game', status: 'observed',
      }],
      objects: [{
        object_id: 'new-controller', role: 'reflex_target', affordances: [],
        loop_class_token: 'new-loop', state: { enabled: 'false', reflex_occurrence: '0' },
      }, {
        object_id: 'first-target', role: 'reflex_target', affordances: ['click'],
        action_token: 'first-token', loop_class_token: 'new-loop',
        state: { enabled: 'true', reflex_occurrence: '0' },
      }],
    },
  }]);
  const [targetCommand] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(targetCommand.payload.document_id, 'document-2');
  assert.equal(targetCommand.payload.object_id, 'first-target');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: targetCommand.command_id,
    result: { accepted: true, dispatch_document_id: 'document-2', dispatch_basis_revision: 1 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-2', base_revision: 1, revision: 2,
      viewport_revision: 1,
      objects: [{
        object_id: 'new-controller', role: 'reflex_target', affordances: [],
        loop_class_token: 'new-loop', state: { enabled: 'false', reflex_occurrence: '1' },
      }],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'new-controller', object_revision: 2 },
        { kind: 'disappeared', object_id: 'first-target', object_revision: 1 },
      ],
    },
  }]);
  const report = await pending;
  assert.equal(report.document_id, 'document-2');
  assert.equal(report.actions, 1);
  assert.equal(report.stop_reason, 'max_actions');
  assert.equal(report.semantic_postcondition.verified, true);
});

test('bounded reflex launch accepts a newly appeared controller after same-document routing', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.acceptTruth('observation', {
    ...observation(),
    frames: [{
      frame_id: 'frame-1', document_id: 'document-1',
      document_url: 'https://game.test/', status: 'observed',
    }],
    objects: [{
      object_id: 'start-link', role: 'link', affordances: ['click'],
      action_token: 'start-token', navigation_target: 'https://game.test/game',
    }],
  });
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    max_actions: 1, start_object_id: 'start-link', timeout_ms: 500,
  }, 500, 30);
  const [startCommand] = await broker.pollCommands(connection.connection_id, 50);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: startCommand.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 1,
      frames: [{
        frame_id: 'frame-1', document_id: 'document-1',
        document_url: 'https://game.test/game', status: 'observed',
      }],
      objects: [{
        object_id: 'same-controller', role: 'reflex_target', affordances: [],
        loop_class_token: 'same-loop', state: { enabled: 'false', reflex_occurrence: '0' },
      }, {
        object_id: 'same-target', role: 'reflex_target', affordances: ['click'],
        action_token: 'same-token', loop_class_token: 'same-loop',
        state: { enabled: 'true', reflex_occurrence: '0' },
      }],
      authorities: [],
      changes: [
        { kind: 'disappeared', object_id: 'start-link', object_revision: 1 },
        { kind: 'appeared', object_id: 'same-controller', object_revision: 2 },
        { kind: 'appeared', object_id: 'same-target', object_revision: 2 },
      ],
    },
  }]);
  const [targetCommand] = await broker.pollCommands(connection.connection_id, 50);
  assert.equal(targetCommand.payload.document_id, 'document-1');
  assert.equal(targetCommand.payload.object_id, 'same-target');
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: targetCommand.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 2 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 2, revision: 3,
      viewport_revision: 1,
      objects: [{
        object_id: 'same-controller', role: 'reflex_target', affordances: [],
        loop_class_token: 'same-loop', state: { enabled: 'false', reflex_occurrence: '1' },
      }],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'same-controller', object_revision: 3 },
        { kind: 'disappeared', object_id: 'same-target', object_revision: 2 },
      ],
    },
  }]);
  const report = await pending;
  assert.equal(report.document_id, 'document-1');
  assert.equal(report.actions, 1);
  assert.equal(report.stop_reason, 'max_actions');
  assert.equal(report.semantic_postcondition.verified, true);
});

test('action receipt excludes unrelated page geometry changes and authorities', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  const full = observation();
  full.objects.push({
    object_id: 'other-object', role: 'button', name: 'Other',
    affordances: ['click'], action_token: 'other-token',
  });
  broker.acceptTruth('observation', full);
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const pending = broker.rpc(session, 'act', {
    tab_id: '7', document_id: 'document-1', basis_revision: 1,
    object_id: 'object-1', operation: 'click', timeout_ms: 200,
  }, 200, 23);
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id,
    result: { accepted: true, dispatch_document_id: 'document-1', dispatch_basis_revision: 1 },
  }, {
    kind: 'observation.delta', payload: {
      tab_id: '7', document_id: 'document-1', base_revision: 1, revision: 2,
      viewport_revision: 2,
      objects: [
        { object_id: 'object-1', role: 'button', name: 'Continue', affordances: ['click'], action_token: 'token-1' },
        { object_id: 'other-object', role: 'button', name: 'Other', affordances: ['click'], action_token: 'other-token' },
      ],
      authorities: [],
      changes: [
        { kind: 'updated', object_id: 'object-1', object_revision: 2 },
        { kind: 'updated', object_id: 'other-object', object_revision: 2 },
      ],
    },
  }]);
  const receipt = await pending;
  assert.deepEqual(receipt.relevant_delta.objects.map((object) => object.object_id), ['object-1']);
  assert.deepEqual(receipt.relevant_delta.changes.map((change) => change.object_id), ['object-1']);
  assert.deepEqual(receipt.relevant_delta.authorities || [], []);
});

test('doctor exposes bounded machine diagnostics without page data', () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.record({ stage: 'extension_queue', code: 'deadline_exceeded', message_bytes: 123 });
  const report = broker.doctor();
  assert.equal(report.runtime, 'node');
  assert.equal(report.active_leases, 1);
  assert.equal(report.online_extension_connections, 0);
  assert.equal(report.extension_polls, 0);
  assert.equal(report.extension_poll_waiters, 0);
  assert.equal(report.extension_keepalives, 0);
  assert.equal(report.extension_keepalive_connections, 0);
  assert.equal(report.recent_failures.at(-1).code, 'deadline_exceeded');
  assert.doesNotMatch(JSON.stringify(report), /objects|cookies|storage|screenshot|action_token/);
});

test('Extension routes accept only a Chrome-extension origin shape', () => {
  assert.equal(extensionOrigin({ headers: {} }), null);
  assert.equal(extensionOrigin({ headers: { origin: 'https://example.test' } }), null);
  assert.equal(extensionOrigin({ headers: { origin: 'chrome-extension://short' } }), null);
  assert.equal(
    extensionOrigin({ headers: { origin: 'chrome-extension://abcdefghijklmnopabcdefghijklmnop' } }),
    'chrome-extension://abcdefghijklmnopabcdefghijklmnop',
  );
});

test('Extension WebSocket heartbeat is origin-bound and value-free', async (context) => {
  const broker = new BrokerState();
  const connected = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const runtime = createBrokerServer(broker, { port: 0 });
  try { await runtime.listen(); } catch (error) {
    if (error.code === 'EPERM') return context.skip('sandbox forbids loopback listen');
    throw error;
  }
  context.after(() => new Promise((resolve) => runtime.server.close(resolve)));
  const address = runtime.server.address();
  const webSocket = new WebSocket(
    `ws://127.0.0.1:${address.port}/v1/extension/keepalive?connection_id=${connected.connection_id}`,
    { headers: { Origin: 'chrome-extension://abcdefghijklmnopabcdefghijklmnop' } },
  );
  await once(webSocket, 'open');
  webSocket.send(JSON.stringify({ kind: 'heartbeat' }));
  const [data] = await once(webSocket, 'message');
  assert.deepEqual(JSON.parse(data.toString('utf8')), {
    kind: 'heartbeat.ack', broker_epoch: broker.epoch,
  });
  assert.equal(broker.doctor().extension_keepalives, 1);
  assert.equal(broker.doctor().extension_keepalive_connections, 1);
  webSocket.close();
  await once(webSocket, 'close');
});
