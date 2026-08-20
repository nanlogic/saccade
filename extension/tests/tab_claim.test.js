'use strict';

// Lifecycle tests for the provisioned Agent-client tab claim. They load the
// real Service Worker source in a sandboxed realm behind a recording Chrome
// double, so every assertion is about shipped behavior rather than a model of
// it. The double fails any call that would enumerate or read a tab the claim is
// not entitled to, which is how the "no scanning" invariant is enforced.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { webcrypto } = require('node:crypto');

const SRC = path.join(__dirname, '..', 'src');
const CANDIDATE = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'candidate.json'), 'utf8'));

function listeners() {
  const registered = [];
  return {
    addListener: (fn) => registered.push(fn),
    emit: (...args) => registered.map((fn) => fn(...args)),
    count: () => registered.length,
  };
}

function createChromeDouble() {
  const calls = [];
  const tabs = new Map();
  const storage = new Map();
  const sessionStorage = new Map();
  let nextTabId = 100;
  let port;

  const events = {
    tabsCreated: listeners(),
    tabsRemoved: listeners(),
    tabsUpdated: listeners(),
    windowsRemoved: listeners(),
    alarm: listeners(),
    runtimeMessage: listeners(),
    runtimeConnect: listeners(),
    startup: listeners(),
    installed: listeners(),
  };

  const chrome = {
    runtime: {
      lastError: undefined,
      getManifest: () => ({ name: 'Saccade (Development)' }),
      getURL: (file) => `chrome-extension://abcdefghijklmnopabcdefghijklmnop/${file}`,
      connectNative: () => {
        port = {
          posted: [],
          disconnectListeners: [],
          messageListeners: [],
          postMessage(message) { this.posted.push(message); },
          onDisconnect: { addListener(fn) { port.disconnectListeners.push(fn); } },
          onMessage: { addListener(fn) { port.messageListeners.push(fn); } },
        };
        return port;
      },
      reload: () => { throw new Error('candidate reload must not happen in tests'); },
      onMessage: { addListener: events.runtimeMessage.addListener },
      onConnect: { addListener: events.runtimeConnect.addListener },
      onStartup: { addListener: events.startup.addListener },
      onInstalled: { addListener: events.installed.addListener },
    },
    storage: {
      local: {
        get: async (key) => (storage.has(key) ? { [key]: storage.get(key) } : {}),
        set: async (entries) => { for (const [key, value] of Object.entries(entries)) storage.set(key, value); },
        remove: async (key) => { storage.delete(key); },
      },
      session: {
        get: async (key) => (sessionStorage.has(key) ? { [key]: sessionStorage.get(key) } : {}),
        set: async (entries) => { for (const [key, value] of Object.entries(entries)) sessionStorage.set(key, value); },
        remove: async (key) => { sessionStorage.delete(key); },
      },
    },
    tabs: {
      async get(tabId) {
        calls.push({ call: 'tabs.get', tabId });
        const tab = tabs.get(tabId);
        if (!tab) throw new Error('No tab with id');
        return { ...tab };
      },
      async query(filter) {
        calls.push({ call: 'tabs.query', filter });
        return [...tabs.values()].filter((tab) => (filter.windowId === undefined || tab.windowId === filter.windowId));
      },
      async create() { throw new Error('tabs.create must not be used by a claim'); },
      async remove(tabId) { calls.push({ call: 'tabs.remove', tabId }); tabs.delete(tabId); },
      async reload(tabId) { calls.push({ call: 'tabs.reload', tabId }); },
      async sendMessage(tabId, message) {
        calls.push({ call: 'tabs.sendMessage', tabId, kind: message.kind });
        if (message.kind === 'collector.ping') return { ok: true, extension_candidate: CANDIDATE };
        if (message.kind === 'collector.configure') return { ok: true };
        return { ok: true };
      },
      onCreated: { addListener: events.tabsCreated.addListener },
      onRemoved: { addListener: events.tabsRemoved.addListener },
      onUpdated: { addListener: events.tabsUpdated.addListener },
    },
    windows: {
      WINDOW_ID_NONE: -1,
      async getAll() { calls.push({ call: 'windows.getAll' }); return [{ id: 1, focused: true }]; },
      async get() { return { id: 1, focused: true }; },
      async create() { throw new Error('windows.create must not be used by a claim'); },
      async update() {},
      onRemoved: { addListener: events.windowsRemoved.addListener },
    },
    alarms: {
      create: () => {},
      clear: () => {},
      onAlarm: { addListener: events.alarm.addListener },
    },
  };

  return {
    chrome,
    calls,
    events,
    port: () => port,
    openTab({ url, id, windowId = 1, openerTabId }) {
      const tabId = id ?? (nextTabId += 1);
      const tab = { id: tabId, url, windowId, active: true, title: 'fixture', openerTabId };
      tabs.set(tabId, tab);
      events.tabsCreated.emit({ ...tab });
      return tabId;
    },
    // A tab the browser reports before its URL settles, as Chrome does for a
    // tab created by another automation client.
    openPendingTab({ pendingUrl, id, windowId = 1 }) {
      const tabId = id ?? (nextTabId += 1);
      const tab = { id: tabId, url: '', pendingUrl, windowId, active: true, title: '' };
      tabs.set(tabId, tab);
      events.tabsCreated.emit({ ...tab, url: undefined });
      return tabId;
    },
    settle(tabId, url) {
      const tab = tabs.get(tabId);
      tab.url = url;
      delete tab.pendingUrl;
      events.tabsUpdated.emit(tabId, { url, status: 'complete' }, { ...tab });
    },
    removeTab(tabId) { tabs.delete(tabId); events.tabsRemoved.emit(tabId); },
    tabs,
  };
}

async function loadWorker() {
  const world = createChromeDouble();
  const sandbox = {
    console: { error: () => {}, warn: () => {}, log: () => {} },
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    crypto: webcrypto,
    URL,
    TextEncoder,
    TextDecoder,
    navigator: { userAgent: 'Mozilla/5.0 Chrome/130.0.0.0' },
    chrome: world.chrome,
    fetch: async () => ({ json: async () => CANDIDATE }),
  };
  const context = vm.createContext(sandbox);
  context.globalThis = context;
  context.importScripts = (...files) => {
    for (const file of files) {
      vm.runInContext(fs.readFileSync(path.join(SRC, file), 'utf8'), context, { filename: file });
    }
  };
  vm.runInContext(
    fs.readFileSync(path.join(SRC, 'service_worker.js'), 'utf8'),
    context,
    { filename: 'service_worker.js' },
  );
  // Let the load-time connectHost() promise chain settle.
  for (let tick = 0; tick < 20; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  return { world, context };
}

let nextRequestId = 0;
async function hostCommand(world, kind, payload = {}) {
  const requestId = nextRequestId += 1;
  const port = world.port();
  port.messageListeners.forEach((fn) => fn({
    protocol: 'saccade-extension-host/1', kind, payload, request_id: requestId,
  }));
  for (let tick = 0; tick < 40; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  const reply = port.posted.find((message) => message.request_id === requestId);
  assert.ok(reply, `no reply for ${kind}`);
  return reply.payload;
}

async function popupCommand(world, kind, tabId) {
  return new Promise((resolve) => {
    world.events.runtimeMessage.emit(
      { kind, tab_id: String(tabId) },
      { url: world.chrome.runtime.getURL('popup.html') },
      resolve,
    );
  });
}

// Values cross a vm realm boundary, so compare structure, not prototypes.
function plain(value) { return JSON.parse(JSON.stringify(value)); }

async function listTabs(world) {
  return plain((await hostCommand(world, 'tabs.list')).tabs);
}

async function arm(world, url) {
  return hostCommand(world, 'tabs.open', { url, claim: 'arm' });
}

test('arm creates, reads, and authorizes nothing', async () => {
  const { world } = await loadWorker();
  world.calls.length = 0;
  const armed = await arm(world, 'https://fixture.test/checkout');
  assert.equal(armed.claim, 'armed');
  assert.equal(armed.origin, 'https://fixture.test');
  assert.match(armed.claim_id, /^claim\.[0-9a-f]{48}$/);
  assert.ok(armed.expires_in_ms > 0);
  assert.equal(armed.tab_id, undefined);
  // No tab was created, queried, read, or messaged while arming.
  assert.deepEqual(plain(world.calls), []);
  assert.deepEqual(await listTabs(world), []);
});

test('only the first matching new tab is latched and confirm yields agent_client provenance', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const first = world.openTab({ url: 'https://fixture.test/checkout' });
  const second = world.openTab({ url: 'https://fixture.test/other' });

  // The second candidate cannot be confirmed by the same claim.
  const stolen = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/other', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(second),
  });
  assert.equal(stolen.error, 'tab claim could not be confirmed');

  const rearmed = await arm(world, 'https://fixture.test/checkout');
  const third = world.openTab({ url: 'https://fixture.test/checkout' });
  const confirmed = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: rearmed.claim_id, tab_id: String(third),
  });
  assert.equal(confirmed.claim, 'confirmed');
  assert.equal(confirmed.tab_id, String(third));
  assert.equal(confirmed.opened, false);
  assert.equal(confirmed.provenance, 'agent_client');

  const listed = await listTabs(world);
  assert.deepEqual(listed.map((tab) => tab.tab_id), [String(third)]);
  assert.equal(listed[0].provenance, 'agent_client');
  assert.equal(listed[0].ownership, 'agent');
  // The other tabs opened during the window stayed Agent Off.
  assert.ok(!listed.some((tab) => tab.tab_id === String(first) || tab.tab_id === String(second)));
});

test('a tab whose URL settles after creation is latched only when the origin matches', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const stranger = world.openPendingTab({ pendingUrl: 'https://elsewhere.test/' });
  world.settle(stranger, 'https://elsewhere.test/');
  // A non-matching tab is decided once and never becomes claimable, even if it
  // later navigates onto the armed origin.
  world.settle(stranger, 'https://fixture.test/checkout');

  const wanted = world.openPendingTab({ pendingUrl: 'https://fixture.test/checkout' });
  world.settle(wanted, 'https://fixture.test/checkout');

  const strangerReply = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(stranger),
  });
  assert.equal(strangerReply.error, 'tab claim could not be confirmed');

  const rearmed = await arm(world, 'https://fixture.test/checkout');
  const fresh = world.openPendingTab({ pendingUrl: 'https://fixture.test/checkout' });
  world.settle(fresh, 'https://fixture.test/checkout');
  const confirmed = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: rearmed.claim_id, tab_id: String(fresh),
  });
  assert.equal(confirmed.provenance, 'agent_client');
});

test('a pre-existing tab on the armed origin is never claimable', async () => {
  const { world } = await loadWorker();
  const existing = world.openTab({ url: 'https://fixture.test/checkout' });
  const armed = await arm(world, 'https://fixture.test/checkout');
  const reply = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(existing),
  });
  assert.equal(reply.error, 'tab claim could not be confirmed');
  assert.deepEqual(await listTabs(world), []);
});

// A tab created by an Agent client starts on a browser-internal page such as
// chrome://newtab/ or about:blank and only reaches the armed origin on a later
// navigation. Those initial pages are not a settled URL, so the candidate must
// survive them; the single origin decision happens on the first HTTP(S) URL.
for (const blank of ['chrome://newtab/', 'about:blank', '']) {
  test(`a tab created at ${blank || 'an empty URL'} is claimable once it reaches the armed origin`, async () => {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: blank });
    world.settle(tabId, 'https://fixture.test/checkout');
    const reply = await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    assert.equal(reply.claim, 'confirmed');
    assert.equal(reply.tab_id, String(tabId));
    assert.equal(reply.provenance, 'agent_client');
    assert.equal(reply.opened, false);
  });
}

test('the first HTTP(S) URL is the only origin decision; returning to the origin gets no second chance', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const tabId = world.openTab({ url: 'chrome://newtab/' });
  world.settle(tabId, 'https://elsewhere.test/landing'); // decided here: wrong origin
  world.settle(tabId, 'https://fixture.test/checkout'); // too late, candidate is gone
  const reply = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
  });
  assert.equal(reply.error, 'tab claim could not be confirmed');
  assert.deepEqual(await listTabs(world), []);
});

test('among several pending tabs only the first to reach the armed origin is claimable', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const first = world.openTab({ url: 'chrome://newtab/' });
  const second = world.openTab({ url: 'about:blank' });
  const third = world.openTab({ url: '' });
  world.settle(second, 'https://fixture.test/checkout'); // wins the latch
  world.settle(first, 'https://fixture.test/checkout');
  world.settle(third, 'https://fixture.test/checkout');
  for (const loser of [first, third]) {
    const failure = await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(loser),
    });
    assert.equal(failure.error, 'tab claim could not be confirmed', `tab ${loser}`);
    const rearmed = await arm(world, 'https://fixture.test/checkout');
    armed.claim_id = rearmed.claim_id; // each mismatch consumes the claim
  }
});

test('a pending tab that reaches the armed origin after the TTL is not claimable', async () => {
  const { world, context } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const tabId = world.openTab({ url: 'chrome://newtab/' });
  vm.runInContext('globalThis.__nowShift = 0; Date.now = ((now) => () => now() + globalThis.__nowShift)(Date.now);', context);
  vm.runInContext('globalThis.__nowShift = 31000;', context);
  world.settle(tabId, 'https://fixture.test/checkout');
  const reply = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
  });
  assert.equal(reply.error, 'tab claim could not be confirmed');
  assert.deepEqual(await listTabs(world), []);
});

test('pending tabs opened with no claim armed stay Agent Off', async () => {
  const { world } = await loadWorker();
  const tabId = world.openTab({ url: 'chrome://newtab/' });
  world.settle(tabId, 'https://fixture.test/checkout');
  const other = world.openTab({ url: 'about:blank' });
  world.settle(other, 'https://elsewhere.test/inbox');
  assert.deepEqual(await listTabs(world), []);
});

test('every mismatch fails uniformly and consumes the single-use claim', async () => {
  const cases = [
    ['wrong claim token', (armed, tabId) => ({ claim_id: 'claim.deadbeef', tab_id: String(tabId), url: 'https://fixture.test/checkout' })],
    ['wrong tab identity', (armed, tabId) => ({ claim_id: armed.claim_id, tab_id: String(tabId + 4242), url: 'https://fixture.test/checkout' })],
    ['wrong origin', (armed, tabId) => ({ claim_id: armed.claim_id, tab_id: String(tabId), url: 'https://elsewhere.test/checkout' })],
    ['missing tab identity', (armed) => ({ claim_id: armed.claim_id, url: 'https://fixture.test/checkout' })],
  ];
  for (const [name, build] of cases) {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
    const failure = await hostCommand(world, 'tabs.open', { claim: 'confirm', ...build(armed, tabId) });
    assert.equal(failure.error, 'tab claim could not be confirmed', name);
    assert.deepEqual(await listTabs(world), [], name);
    // The claim is consumed: the correct confirm no longer works.
    const retry = await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    assert.equal(retry.error, 'tab claim could not be confirmed', `${name} retry`);
    assert.deepEqual(await listTabs(world), [], `${name} retry`);
  }
});

test('a claim expires after its short TTL', async () => {
  const { world, context } = await loadWorker();
  assert.equal(vm.runInContext('CLAIM_TTL_MS', context), 30_000);
  const armed = await arm(world, 'https://fixture.test/checkout');
  const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
  const realNow = Date.now;
  vm.runInContext('globalThis.__nowShift = 0; Date.now = ((now) => () => now() + globalThis.__nowShift)(Date.now);', context);
  vm.runInContext('globalThis.__nowShift = 31000;', context);
  const failure = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
  });
  assert.equal(failure.error, 'tab claim could not be confirmed');
  assert.deepEqual(await listTabs(world), []);
  assert.equal(Date.now, realNow);
});

test('re-arming replaces any earlier unconsumed claim', async () => {
  const { world } = await loadWorker();
  const first = await arm(world, 'https://fixture.test/checkout');
  const firstTab = world.openTab({ url: 'https://fixture.test/checkout' });
  const second = await arm(world, 'https://fixture.test/checkout');
  assert.notEqual(first.claim_id, second.claim_id);
  const failure = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: first.claim_id, tab_id: String(firstTab),
  });
  assert.equal(failure.error, 'tab claim could not be confirmed');
});

test('an unsupported claim mode is rejected outright', async () => {
  const { world } = await loadWorker();
  const failure = await hostCommand(world, 'tabs.open', { url: 'https://fixture.test/', claim: 'adopt' });
  assert.equal(failure.error, 'claim must be arm or confirm');
});

test('a claimed tab is revoked by Stop sharing, tab removal, and host disconnect', async () => {
  // Stop sharing.
  {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
    await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    const status = await popupCommand(world, 'ui.tab.status', tabId);
    assert.equal(status.ok, true);
    assert.equal(status.status.authorized, true);
    assert.equal(status.status.provenance, 'agent_client');
    const revoked = await popupCommand(world, 'ui.tab.revoke', tabId);
    assert.equal(revoked.status.authorized, false);
    assert.equal(revoked.status.provenance, 'none');
    assert.deepEqual(await listTabs(world), []);
    assert.ok(world.tabs.has(tabId), 'revoking access must not close the tab');
  }

  // Tab removed by the user or by tabs.close.
  {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
    await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    const closed = await hostCommand(world, 'tabs.close', { tab_id: String(tabId) });
    assert.equal(closed.closed, true);
    assert.deepEqual(await listTabs(world), []);
  }

  // The tab disappears without Saccade involvement.
  {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
    await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    world.removeTab(tabId);
    for (let tick = 0; tick < 20; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(await listTabs(world), []);
  }

  // Native Host session disconnect ends the claimed authority.
  {
    const { world } = await loadWorker();
    const armed = await arm(world, 'https://fixture.test/checkout');
    const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
    await hostCommand(world, 'tabs.open', {
      url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
    });
    const port = world.port();
    port.disconnectListeners.forEach((fn) => fn());
    for (let tick = 0; tick < 40; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
    const status = await popupCommand(world, 'ui.tab.status', tabId);
    assert.equal(status.status.authorized, false);
    assert.equal(status.status.provenance, 'none');
    assert.ok(world.tabs.has(tabId), 'host disconnect must not close the tab');
  }
});

test('a delayed browser startup event cannot revoke a claim created after Host readiness', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/checkout');
  const tabId = world.openTab({ url: 'https://fixture.test/checkout' });
  await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/checkout', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(tabId),
  });
  world.events.startup.emit();
  for (let tick = 0; tick < 40; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  const status = await popupCommand(world, 'ui.tab.status', tabId);
  assert.equal(status.status.authorized, true);
  assert.equal(status.status.provenance, 'agent_client');
});

test('ordinary user tabs stay Agent Off and user_shared lifecycle is unaffected', async () => {
  const { world } = await loadWorker();
  const userTab = world.openTab({ url: 'https://fixture.test/inbox' });
  const otherTab = world.openTab({ url: 'https://elsewhere.test/inbox' });
  assert.deepEqual(await listTabs(world), []);

  // Sharing still works exactly as before, with user_shared provenance.
  const shared = await popupCommand(world, 'ui.tab.share', userTab);
  assert.equal(shared.ok, true);
  assert.equal(shared.status.provenance, 'user_shared');
  assert.equal(shared.status.authorized, true);

  // A claim window never touches the shared tab, and a claim cannot capture it.
  const armed = await arm(world, 'https://fixture.test/inbox');
  const stolen = await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/inbox', claim: 'confirm', claim_id: armed.claim_id, tab_id: String(userTab),
  });
  assert.equal(stolen.error, 'tab claim could not be confirmed');

  const listed = await listTabs(world);
  assert.deepEqual(listed.map((tab) => tab.tab_id), [String(userTab)]);
  assert.equal(listed[0].provenance, 'user_shared');
  assert.ok(!listed.some((tab) => tab.tab_id === String(otherTab)));

  // Saccade still refuses to close a user_shared tab.
  const refused = await hostCommand(world, 'tabs.close', { tab_id: String(userTab) });
  assert.equal(refused.error, 'only Agent-owned tabs may be closed through Saccade');
  assert.ok(world.tabs.has(userTab));

  // A host disconnect revokes claims only; user_shared survives.
  const port = world.port();
  port.disconnectListeners.forEach((fn) => fn());
  for (let tick = 0; tick < 40; tick += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  const status = await popupCommand(world, 'ui.tab.status', userTab);
  assert.equal(status.status.authorized, true);
  assert.equal(status.status.provenance, 'user_shared');
});

test('a new tab never inherits Agent On from its opener', async () => {
  const { world } = await loadWorker();
  const armed = await arm(world, 'https://fixture.test/agent');
  const agentTab = world.openTab({ url: 'https://fixture.test/agent' });
  await hostCommand(world, 'tabs.open', {
    url: 'https://fixture.test/agent', claim: 'confirm',
    claim_id: armed.claim_id, tab_id: String(agentTab),
  });

  const userTab = world.openTab({
    url: 'https://fixture.test/user-opened', openerTabId: agentTab,
  });
  const status = await popupCommand(world, 'ui.tab.status', userTab);
  assert.equal(status.status.authorized, false);
  assert.equal(status.status.provenance, 'none');
  assert.deepEqual((await listTabs(world)).map((tab) => tab.tab_id), [String(agentTab)]);
});

test('observation resync targets one authorized tab and never scans other tabs', async () => {
  const { world } = await loadWorker();
  const target = world.openTab({ url: 'https://fixture.test/target' });
  const unrelated = world.openTab({ url: 'https://fixture.test/unrelated' });
  assert.equal((await popupCommand(world, 'ui.tab.share', target)).ok, true);

  const result = await hostCommand(world, 'observation.resync', { tab_id: String(target) });
  assert.deepEqual(plain(result), { tab_id: String(target), resync_requested: true });
  const snapshotCalls = world.calls.filter((call) => call.kind === 'collector.snapshot');
  assert.deepEqual(snapshotCalls, [{ call: 'tabs.sendMessage', tabId: target, kind: 'collector.snapshot' }]);
  assert.ok(!snapshotCalls.some((call) => call.tabId === unrelated));
});

test('the claim is one generic Chrome/Edge codepath with no browser branch', () => {
  const worker = fs.readFileSync(path.join(SRC, 'service_worker.js'), 'utf8');
  const claimRegion = worker.slice(worker.indexOf('function armTabClaim'), worker.indexOf('async function authorizeTab'));
  assert.ok(claimRegion.includes('armTabClaim'));
  assert.ok(claimRegion.includes('confirmTabClaim'));
  for (const branch of ['BROWSER_FAMILY', 'userAgent', 'Edg/', 'edge', 'chrome.debugger', 'Playwright', 'captureVisibleTab']) {
    assert.ok(!claimRegion.includes(branch), `claim path branched on ${branch}`);
  }
  // The Extension gained no execution capability alongside the claim.
  assert.ok(!worker.includes('chrome.debugger'));
  assert.ok(!worker.includes('captureVisibleTab'));
  assert.ok(!worker.includes('chrome.scripting'));
  // Nothing in the claim path names a model or vendor.
  for (const vendor of ['Claude', 'Codex', 'OpenAI', 'Anthropic', 'Gemini']) {
    assert.ok(!worker.includes(vendor), `service worker names ${vendor}`);
  }
});

test('protected-value redaction rules are untouched by the claim', () => {
  const { isProtectedFieldType, redactProtectedText } = require('../src/consent.js');
  assert.equal(isProtectedFieldType('password'), true);
  assert.equal(isProtectedFieldType('text', 'new-password'), true);
  assert.equal(isProtectedFieldType('text', '', 'Employer Identification Number'), true);
  assert.equal(isProtectedFieldType('text', 'cc-number'), false);
  assert.equal(redactProtectedText('SSN 123-45-6789 EIN 12-3456789'), 'SSN [REDACTED SSN] EIN [REDACTED EIN]');
  const collector = fs.readFileSync(path.join(SRC, 'collector.js'), 'utf8');
  assert.ok(!collector.includes('claim'), 'the Collector must not learn about claims');
});
