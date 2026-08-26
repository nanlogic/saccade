'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const { BrokerState, extensionOrigin } = require('../src/broker');

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
      changes: [{ kind: 'updated', object_id: 'name', object_revision: 2 }],
    },
  }]);
  const receipt = await pending;
  assert.equal(receipt.outcome, 'accepted');
  assert.deepEqual(receipt.steps.map((step) => step.object_id), ['name', 'country']);
  assert.doesNotMatch(JSON.stringify(receipt), /secret-not-in-receipt/);
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

test('doctor exposes bounded machine diagnostics without page data', () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  broker.leaseTab('7', session);
  broker.record({ stage: 'extension_queue', code: 'deadline_exceeded', message_bytes: 123 });
  const report = broker.doctor();
  assert.equal(report.runtime, 'node');
  assert.equal(report.active_leases, 1);
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
