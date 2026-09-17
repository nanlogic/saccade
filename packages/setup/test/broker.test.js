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

function connectTestConsumer(broker, payload) {
  const connected = broker.connectExtension(payload);
  // Unit tests drive delivery by calling pollCommands after enqueue. Record
  // the first-poll proof here without creating a waiter that would consume
  // that command before the test can inspect it.
  broker.connections.get(connected.connection_id).last_poll_at = broker.now();
  return connected;
}

test('Extension tab sharing binds explicit online sessions without leaking page content or transferring leases', async () => {
  const broker = new BrokerState();
  const origin = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop';
  const owner = broker.createSession().agent_session_id;
  const other = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' }, origin);
  const request = { connection_id: connection.connection_id, tab_id: '7', operation: 'status' };
  const initial = broker.extensionTabSharing(request, origin);
  assert.equal(initial.state, 'unassigned');
  assert.equal(initial.sessions.length, 2);
  assert.doesNotMatch(JSON.stringify(initial), /resume|token|upload|objects|document/);
  assert.throws(() => broker.extensionTabSharing(request, 'chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'), { code: 'EXTENSION_AUTH_FAILED' });
  assert.throws(() => broker.extensionTabSharing({ ...request, operation: 'assign' }, origin), { code: 'INVALID_REQUEST' });
  const assigned = broker.extensionTabSharing({ ...request, operation: 'assign', agent_session_id: owner }, origin);
  assert.equal(assigned.agent_session_id, owner);
  assert.equal(assigned.state, 'active');
  assert.equal(broker.listTabs(owner)[0].ownership, 'user_shared');
  assert.deepEqual(broker.listTabs(other), []);
  assert.throws(() => broker.extensionTabSharing({ ...request, operation: 'assign', agent_session_id: other }, origin), { code: 'TAB_ALREADY_LEASED' });
  const secondBrowser = broker.connectExtension({ browser_instance_id: 'browser-2' }, origin);
  for (const operation of ['status', 'assign', 'revoke']) {
    assert.throws(() => broker.extensionTabSharing({ ...request, connection_id: secondBrowser.connection_id, operation, agent_session_id: other }, origin), { code: 'TAB_BROWSER_MISMATCH' });
  }
  broker.closeSession(owner);
  assert.equal(broker.extensionTabSharing(request, origin).state, 'orphaned');
  assert.throws(() => broker.extensionTabSharing({ ...request, operation: 'assign', agent_session_id: other }, origin), { code: 'TAB_ALREADY_LEASED' });
  assert.equal(broker.extensionTabSharing({ ...request, operation: 'revoke' }, origin).state, 'unassigned');
  assert.throws(() => broker.extensionTabSharing({ ...request, operation: 'assign', agent_session_id: owner }, origin), { code: 'SESSION_OFFLINE' });
  assert.equal(broker.extensionTabSharing({ ...request, operation: 'assign', agent_session_id: other }, origin).state, 'active');
  broker.disconnectExtension(connection.connection_id);
  assert.throws(() => broker.extensionTabSharing(request, origin), { code: 'EXTENSION_OFFLINE' });
});

test('Extension tab revoke clears Truth and cancels queued work and delivered media without closing tab', async () => {
  const broker = new BrokerState();
  const origin = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop';
  const owner = broker.createSession().agent_session_id;
  const connected = broker.connectExtension({ browser_instance_id: 'browser-1' }, origin);
  const connection = broker.connections.get(connected.connection_id);
  connection.last_poll_at = broker.now();
  const request = { connection_id: connected.connection_id, tab_id: '7', operation: 'assign', agent_session_id: owner };
  broker.extensionTabSharing(request, origin);
  broker.acceptTruth('observation', observation());
  const media = broker.enqueueCommand(owner, 'media.read', { tab_id: '7' }, 1000, { browserInstanceId: 'browser-1' });
  const [delivered] = await broker.pollCommands(connected.connection_id, 10);
  const signals = [];
  connection.keepalive_socket = { send: (value) => signals.push(JSON.parse(value)) };
  const queued = broker.enqueueCommand(owner, 'tabs.close', { tab_id: '7' }, 1000, { browserInstanceId: 'browser-1' });
  const rejected = assert.rejects(queued, { code: 'CANCELLED' });
  broker.extensionTabSharing({ ...request, operation: 'revoke' }, origin);
  await rejected;
  assert.equal(broker.truth.has('7'), false);
  assert.equal(connection.queue.length, 0);
  assert.equal(signals[0].command_id, delivered.command_id);
  const mediaRejected = assert.rejects(media, { code: 'CANCELLED' });
  broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: delivered.command_id, result: {} }]);
  await mediaRejected;
  connection.keepalive_socket = null;
});

test('visual grant duration is validated and echoed only from a matching Extension', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  broker.connections.get(connected.connection_id).visual_consent_version = 1;
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  const scope = { tab_id: '7', document_id: 'document-1', grant_ttl_ms: 10800000 };
  for (const duration of [0, -1, 1.5, 10800001]) {
    await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', { ...scope, grant_ttl_ms: duration }), { code: 'INVALID_REQUEST' });
  }
  await assert.rejects(broker.rpc(owner, 'media.authorization.prepare', scope), { code: 'INVALID_REQUEST' });
  await assert.rejects(broker.rpc(owner, 'visual.authorization.accept', { ...scope, challenge: 'c', source: 'confirmation' }), { code: 'INVALID_REQUEST' });
  for (const duration of [undefined, 900000, 10800001, 10800000]) {
    const task = broker.rpc(owner, 'visual.authorization.prepare', scope, 1000);
    const result = duration === 10800000 ? task : assert.rejects(task, { code: 'VISUAL_CONSENT_DURATION_UPGRADE_REQUIRED' });
    const [command] = await broker.pollCommands(connected.connection_id, 10);
    assert.equal(command.payload.grant_ttl_ms, 10800000);
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: command.command_id,
      result: { granted: false, challenge: 'c', grant_ttl_ms: duration, secret: 'not-returned' } }]);
    const response = await result;
    if (duration === 10800000) assert.deepEqual(response, { granted: false, challenge: 'c', grant_ttl_ms: 10800000 });
  }
  broker.closeSession(owner);
});

test('session visual negotiation validates scope, fixed expiry and exact lease before dispatch', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const other = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  broker.connections.get(connected.connection_id).visual_consent_version = 1;
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  const params = { tab_id: '7', document_id: 'document-1', grant_ttl_ms: 10800000, grant_scope: 'session' };
  for (const patch of [{ grant_scope: 'all' }, { grant_ttl_ms: 900000 }, { grant_expires_at: 0 },
    { grant_expires_at: broker.now() + 10860000 }, { grant_expires_at: 'tomorrow' }]) {
    await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', { ...params, ...patch }), { code: 'INVALID_REQUEST' });
  }
  await assert.rejects(broker.rpc(other, 'visual.authorization.prepare', params));
  await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', { ...params, tab_id: 'private-tab' }));
  await assert.rejects(broker.rpc(owner, 'media.authorization.prepare', params), { code: 'INVALID_REQUEST' });
  assert.equal(broker.commands.size, 0);
  const dispatch = async (method, args, result) => {
    const task = broker.rpc(owner, method, args, 1000);
    const checked = task.then(value => ({ value }), error => ({ error }));
    const [command] = await broker.pollCommands(connected.connection_id, 10);
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: command.command_id, result }]);
    return checked;
  };
  let response = await dispatch('visual.authorization.prepare', params, { granted: false, challenge: 'old', grant_ttl_ms: 10800000 });
  assert.equal(response.error.code, 'VISUAL_SESSION_CONSENT_UPGRADE_REQUIRED');
  const expiry = broker.now() + 900000;
  response = await dispatch('visual.authorization.prepare', { ...params, grant_expires_at: expiry },
    { granted: false, challenge: 'derived', grant_ttl_ms: 10800000, grant_scope: 'session', grant_expires_at: expiry, secret: 'not-returned' });
  assert.deepEqual(response.value, { granted: false, challenge: 'derived', grant_ttl_ms: 10800000, grant_scope: 'session', grant_expires_at: expiry });
  response = await dispatch('visual.authorization.accept', { tab_id: '7', document_id: 'document-1', challenge: 'derived', source: 'session_confirmation' },
    { granted: true, grant_scope: 'session', expires_at: expiry, secret: 'not-returned' });
  assert.deepEqual(response.value, { granted: true, grant_scope: 'session', expires_at: expiry });
  await assert.rejects(broker.rpc(owner, 'visual.read', { tab_id: '7', document_id: 'document-1', grant_scope: 'session' }), { code: 'INVALID_REQUEST' });
  broker.closeSession(owner); broker.closeSession(other);
});

test('authorization distinguishes an unavailable consumer from an old consent protocol', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1',
    media_consent_version: 1, visual_consent_version: 1, session_consent_version: 2 });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  broker.connections.get(connected.connection_id).last_poll_at = null;
  for (const kind of ['media', 'visual']) {
    await assert.rejects(broker.rpc(owner, `${kind}.authorization.prepare`, {
      tab_id: '7', document_id: 'document-1',
    }), { code: 'EXTENSION_OFFLINE' });
  }
  assert.equal(broker.commands.size, 0);
  broker.closeSession(owner);
});

test('unified session negotiation gates both kinds even on existing grants and preserves exact leases', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id, other = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1', media_consent_version: 1, visual_consent_version: 1 });
  const connection = broker.connections.get(connected.connection_id);
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' }); broker.acceptTruth('observation', observation());
  const params = { tab_id: '7', document_id: 'document-1', grant_ttl_ms: 10800000, grant_scope: 'session_observation' };
  const dispatch = async (method, args, result) => {
    const checked = broker.rpc(owner, method, args, 1000).then(value => ({ value }), error => ({ error }));
    const [command] = await broker.pollCommands(connected.connection_id, 10);
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: command.command_id, result }]);
    return checked;
  };
  for (const kind of ['visual','media']) {
    connection.session_consent_version = 0;
    await assert.rejects(broker.rpc(owner, `${kind}.authorization.prepare`, params), { code: `${kind.toUpperCase()}_SESSION_CONSENT_UPGRADE_REQUIRED` });
    connection.session_consent_version = 2;
    await assert.rejects(broker.rpc(other, `${kind}.authorization.prepare`, params));
    await assert.rejects(broker.rpc(owner, `${kind}.authorization.prepare`, { ...params, document_id: 'stale' }));
    const legacy = await dispatch(`${kind}.authorization.prepare`, params, { granted: true });
    assert.equal(legacy.error.code, `${kind.toUpperCase()}_SESSION_CONSENT_UPGRADE_REQUIRED`);
    const prepared = await dispatch(`${kind}.authorization.prepare`, params, { granted: false, challenge: 'new',
      session_consent_version: 2, grant_scope: 'session_observation', grant_ttl_ms: 10800000 });
    assert.equal(prepared.value.session_consent_version, 2);
    const grant = await dispatch(`${kind}.authorization.accept`, { tab_id: '7', document_id: 'document-1', challenge: 'new', source: 'session_confirmation' },
      { granted: true, session_consent_version: 2, grant_scope: 'session_observation', expires_at: broker.now() + 10800000 });
    assert.equal(grant.value.grant_scope, 'session_observation');
  }
  broker.closeSession(owner); broker.closeSession(other);
});

test('visual conversational authorization is version gated, deadline bound and cancelled on session close', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id:'browser-1' });
  const connection = broker.connections.get(connected.connection_id);
  broker.leaseTab('7', owner, { browser_instance_id:'browser-1' });
  broker.acceptTruth('observation', observation());
  const scope = { tab_id:'7', document_id:'document-1' };
  await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', scope, 1000), { code:'VISUAL_CONSENT_UPGRADE_REQUIRED' });
  connection.visual_consent_version = 1;
  const deadline = Date.now()+800;
  const prepared = broker.rpc(owner, 'visual.authorization.prepare', scope, 1000, 'visual-request', deadline);
  const [prepare] = await broker.pollCommands(connected.connection_id, 10);
  assert.equal(prepare.deadline_at, deadline);
  broker.acceptExtensionEvents(connected.connection_id, [{ kind:'response', command_id:prepare.command_id, result:{ granted:false, challenge:'private-challenge' } }]);
  assert.equal((await prepared).challenge, 'private-challenge');
  const accepted = broker.rpc(owner, 'visual.authorization.accept', { ...scope, challenge:'private-challenge', source:'client_confirmation' }, 1000, 'visual-request', deadline);
  const [accept] = await broker.pollCommands(connected.connection_id, 10);
  broker.acceptExtensionEvents(connected.connection_id, [{ kind:'response', command_id:accept.command_id, result:{ granted:true } }]);
  assert.equal((await accepted).granted, true);
  const signals = [];
  connection.keepalive_socket = { send: (value) => signals.push(JSON.parse(value)) };
  broker.cancelRequest(owner, 'visual-request');
  assert(signals.some((item) => item.command_id === accept.command_id));
  assert(signals.some((item) => item.command_id === prepare.command_id));
  signals.length=0;
  broker.closeSession(owner);
  assert(signals.some((item) => item.command_id === accept.command_id));
  connection.keepalive_socket=null;
});

test('visual confirmation retains thirty-second request deadline and snapshot execution stays within ten seconds', async () => {
  let now = Date.now();
  const started = now;
  const deadline = started + 30000;
  const broker = new BrokerState({ now: () => now });
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1', visual_consent_version: 1 });
  const connection = broker.connections.get(connected.connection_id);
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  const scope = { tab_id: '7', document_id: 'document-1' };
  const prepared = broker.rpc(owner, 'visual.authorization.prepare', scope, 30000, 'visual-budget', deadline);
  const [prepare] = await broker.pollCommands(connected.connection_id, 10);
  assert.equal(prepare.deadline_at, deadline);
  broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: prepare.command_id, result: { granted: false, challenge: 'challenge' } }]);
  await prepared;

  // Human confirmation consumes fifteen seconds of the original request.
  now = started + 15000;
  connection.last_poll_at = now;
  const accepted = broker.rpc(owner, 'visual.authorization.accept', { ...scope, challenge: 'challenge', source: 'client_confirmation' }, 30000, 'visual-budget', deadline);
  const [accept] = await broker.pollCommands(connected.connection_id, 10);
  assert.equal(accept.deadline_at, deadline);
  broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: accept.command_id, result: { granted: true } }]);
  assert.equal((await accepted).granted, true);

  for (const elapsed of [15000, 28000]) {
    now = started + elapsed;
    connection.last_poll_at = now;
    const snapshot = broker.rpc(owner, 'visual.read', { ...scope, timeout_ms: 30000 }, 30000, `snapshot-${elapsed}`, deadline);
    const [command] = await broker.pollCommands(connected.connection_id, 10);
    assert.equal(command.deadline_at, Math.min(deadline, now + 10000));
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: command.command_id, result: {
      schema: 'saccade.visual/1', ...scope, image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' },
    } }]);
    await snapshot;
  }
  await assert.rejects(broker.rpc(owner, 'visual.read', { ...scope, timeout_ms: 30001 }, 30000, 'invalid-budget', deadline), { code: 'INVALID_REQUEST' });
  now = deadline + 1;
  await assert.rejects(broker.rpc(owner, 'visual.authorization.accept', { ...scope, challenge: 'challenge', source: 'client_confirmation' }, 30000, 'expired-budget', deadline), { code: 'DEADLINE_EXCEEDED' });
});

test('cancelled delivered visual reads cannot return screenshot pixels', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id:'browser-1' });
  const connection = broker.connections.get(connected.connection_id);
  broker.leaseTab('7', owner, { browser_instance_id:'browser-1' });
  broker.acceptTruth('observation', observation());
  const running = broker.rpc(owner, 'visual.read', { tab_id:'7', document_id:'document-1' }, 1000, 'visual-read');
  const rejected = assert.rejects(running, { code:'CANCELLED' });
  const [command] = await broker.pollCommands(connected.connection_id, 10);
  const signals = [];
  connection.keepalive_socket = { send:(value) => signals.push(JSON.parse(value)) };
  broker.cancelRequest(owner, 'visual-read');
  assert.equal(signals[0].command_id, command.command_id);
  broker.acceptExtensionEvents(connected.connection_id, [{ kind:'response', command_id:command.command_id, result:{ image:{ data:'must-not-return' } } }]);
  await rejected;
  connection.keepalive_socket=null;
});

test('separate consent wait is negotiated, validated and retains cancellation metadata', async () => {
  for (const type of ['visual', 'media']) {
    let now = Date.now();
    const broker = new BrokerState({ now: () => now });
    const owner = broker.createSession().agent_session_id;
    const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1', [`${type}_consent_version`]: 1 });
    const connection = broker.connections.get(connected.connection_id);
    broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
    broker.acceptTruth('observation', observation());
    const scope = { tab_id: '7', document_id: 'document-1' };
    const params = { ...scope, wait_for_consent: true };
    await assert.rejects(broker.rpc(owner, `${type}.authorization.prepare`, params, 30000), { code: `${type.toUpperCase()}_CONSENT_WAIT_UPGRADE_REQUIRED` });
    assert.equal(connection.queue.length, 0);
    connection.consent_wait_version = 1;
    assert.equal((await broker.rpc(owner, 'system.capabilities')).connected_extensions[0].consent_wait_version, 1);
    const pending = broker.rpc(owner, `${type}.authorization.prepare`, params, 30000, 'consent-wait');
    const [prepare] = await broker.pollCommands(connected.connection_id, 10);
    assert.equal(prepare.deadline_at, now + 30000);
    const expires = now + 600000;
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: prepare.command_id, result: {
      granted: false, challenge: 'one-use', consent_wait_version: 1, consent_expires_at: expires, secret: 'must-not-return',
    } }]);
    assert.deepEqual(await pending, { granted: false, challenge: 'one-use', consent_wait_version: 1, consent_expires_at: expires });
    const retained = broker.commands.get(prepare.command_id);
    assert.equal(retained.cleanup_timer._idleTimeout, 600000);
    assert.doesNotMatch(JSON.stringify(retained.payload), /one-use|wait_for_consent/);
    now += 90000;
    connection.last_poll_at = now;
    const accepted = broker.rpc(owner, `${type}.authorization.accept`, { ...scope, challenge: 'one-use', source: 'client_confirmation' }, 30000, 'consent-wait', now + 30000);
    const [accept] = await broker.pollCommands(connected.connection_id, 10);
    assert.equal(accept.deadline_at, now + 30000);
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: accept.command_id, result: { granted: true } }]);
    assert.deepEqual(await accepted, { granted: true });
    const signals = [];
    connection.keepalive_socket = { send: value => signals.push(JSON.parse(value)) };
    broker.cancelRequest(owner, 'consent-wait');
    assert(signals.some(signal => signal.command_id === prepare.command_id));
    assert(signals.some(signal => signal.command_id === accept.command_id));
    connection.keepalive_socket = null;
    await assert.rejects(broker.rpc(owner, `${type}.authorization.accept`, { ...scope, challenge: 'one-use', source: 'confirmation', wait_for_consent: true }), { code: 'INVALID_REQUEST' });
  }
});

test('consent wait rejects malformed Extension expiry instead of exposing or renewing it', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1', visual_consent_version: 1, consent_wait_version: 1 });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  for (const result of [
    { granted: false, challenge: 'one-use' },
    { granted: false, challenge: 'one-use', consent_wait_version: 1, consent_expires_at: Date.now() - 1 },
    { granted: false, challenge: 'one-use', consent_wait_version: 1, consent_expires_at: Date.now() + 700000 },
  ]) {
    const pending = broker.rpc(owner, 'visual.authorization.prepare', { tab_id: '7', document_id: 'document-1', wait_for_consent: true }, 30000);
    const rejected = assert.rejects(pending, { code: 'VISUAL_AUTHORIZATION_INVALID' });
    const [command] = await broker.pollCommands(connected.connection_id, 10);
    broker.acceptExtensionEvents(connected.connection_id, [{ kind: 'response', command_id: command.command_id, result }]);
    await rejected;
  }
});

test('consent wait handshake reports only the negotiated numeric version and public reads reject private wait fields', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  connectTestConsumer(broker, { browser_instance_id: 'browser-1', consent_wait_version: 1 });
  connectTestConsumer(broker, { browser_instance_id: 'browser-2', consent_wait_version: '1' });
  const capabilities = await broker.rpc(owner, 'system.capabilities');
  assert.equal(capabilities.connected_extensions.find(item => item.browser_instance_id === 'browser-1').consent_wait_version, 1);
  assert.equal(capabilities.connected_extensions.find(item => item.browser_instance_id === 'browser-2').consent_wait_version, 0);
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', observation());
  for (const type of ['visual', 'media']) {
    await assert.rejects(broker.rpc(owner, `${type}.read`, { tab_id: '7', document_id: 'document-1', wait_for_consent: true, ...(type === 'media' ? { mode: 'catalog' } : {}) }), { code: 'INVALID_REQUEST' });
  }
});

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

test('scene sequence binds generation and keeps identity in complete compact catalog',async()=>{
 const broker=new BrokerState(),owner=broker.createSession().agent_session_id;
 const connection=connectTestConsumer(broker,{browser_instance_id:'browser-1',scene_version:1});
 broker.leaseTab('7',owner,{browser_instance_id:'browser-1'});
 const snapshot=observation();snapshot.objects=Array.from({length:65},(_,i)=>({object_id:`scene.${i}`,role:'scene_object',kind:'scene_object',scene_generation:'actor-1',source:'application_reported',state:{speed:1},affordances:[]}));
 broker.acceptTruth('observation',snapshot);
 const read=await broker.rpc(owner,'truth.read',{tab_id:'7',mode:'full'});
 assert.equal(read.catalog,'complete_compact');assert.equal(read.objects[0].scene_generation,'actor-1');assert.equal(read.objects[0].source,'application_reported');
 const request={tab_id:'7',document_id:'document-1',object_id:'scene.0',scene_generation:'actor-1',mode:'sequence',duration_ms:100};
 await assert.rejects(broker.rpc(owner,'visual.read',{...request,scene_generation:'wrong'}),{code:'INVALID_REQUEST'});
 await assert.rejects(broker.rpc(owner,'visual.read',{...request,duration_ms:3001}),{code:'INVALID_REQUEST'});
 const running=broker.rpc(owner,'visual.read',request,1000);
 const result=await deliver(broker,connection.connection_id,running,{schema:'saccade.visual-sequence/1',tab_id:'7',document_id:'document-1',object_id:'scene.0',scene_generation:'actor-1',frames:[{frame_id:1,elapsed_ms:1,simulation_time_ms:16,image:{type:'image',mimeType:'image/webp',data:'YQ=='}}]});
 assert.equal(result.frames.length,1);
});

test('tabs.open atomically leases one tab to one Agent session', async () => {
  const broker = new BrokerState();
  const first = broker.createSession().agent_session_id;
  const second = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const opened = broker.rpc(first, 'tabs.open', { url: 'https://example.test' }, 1000);
  const result = await deliver(broker, connection.connection_id, opened, { tab_id: '7', opened: true });
  assert.equal(result.tab_id, '7');
  assert.equal(result.agent_session_id, first);
  assert.deepEqual(broker.listTabs(first).map((tab) => tab.tab_id), ['7']);
  assert.deepEqual(broker.listTabs(second), []);
  assert.throws(() => broker.requireLease('7', second), /another Agent/);
});

test('visual requests enforce ownership, document, size and exact response routing', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const other = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  await deliver(broker, connection.connection_id,
    broker.rpc(owner, 'tabs.open', { url: 'https://example.test' }, 1000), { tab_id: '7', opened: true });
  const args = { tab_id: '7', document_id: 'document-1' };
  await assert.rejects(broker.rpc(other, 'visual.read', args), /another Agent/);
  await assert.rejects(broker.rpc(owner, 'visual.read', { ...args, document_id: 'old' }), /Current document/);
  await assert.rejects(broker.rpc(owner, 'visual.read', { ...args, max_width: 9000 }), /Invalid screenshot/);
  await assert.rejects(broker.rpc(owner, 'visual.read', { ...args, object_id: 'missing' }), /Invalid screenshot/);
  const valid = { schema: 'saccade.visual/1', ...args, image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' } };
  const pending = broker.rpc(owner, 'visual.read', args, 1000);
  const result = await deliver(broker, connection.connection_id, pending, valid);
  assert.equal(result.image.data, 'YQ==');
  assert.equal(JSON.stringify(broker.truth.get('7')).includes('YQ=='), false);
  const wrongTab = broker.rpc(owner, 'visual.read', args, 1000);
  await assert.rejects(deliver(broker, connection.connection_id, wrongTab, { ...valid, tab_id: '8' }), /Screenshot document/);
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
  const chrome = connectTestConsumer(broker, { browser_instance_id: 'browser-chrome' });
  const edge = connectTestConsumer(broker, { browser_instance_id: 'browser-edge' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const first = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'act', { tab_id: '7' }, 1000);
  const [command] = await broker.pollCommands(first.connection_id, 10);
  assert.equal(command.kind, 'act');
  broker.disconnectExtension(first.connection_id, 'power_loss');
  await assert.rejects(pending, (error) => error.code === 'OUTCOME_UNKNOWN' && error.retry_safe === false);
  const second = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  assert.deepEqual(await broker.pollCommands(second.connection_id, 5), []);
});

test('queued work belongs to the browser and is claimed only on Extension delivery', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const first = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'tabs.open', { url: 'https://example.test' }, 1000);
  const queued = [...broker.commands.values()].at(-1);
  assert.equal(queued.browser_instance_id, 'browser-1');
  assert.equal(queued.connection_id, null);

  const second = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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

test('queued work behind a delivered action follows the exact browser replacement', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const first = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const dispatched = broker.enqueueCommand(session, 'act', { tab_id: '7' }, 1_000);
  const [action] = await broker.pollCommands(first.connection_id, 10);
  assert.equal(action.kind, 'act');

  const queued = broker.enqueueCommand(session, 'tabs.open', {}, 1_000);
  const replacement = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  await assert.rejects(dispatched, (error) => error.code === 'OUTCOME_UNKNOWN'
    && error.retry_safe === false);
  const [open] = await broker.pollCommands(replacement.connection_id, 10);
  assert.equal(open.kind, 'tabs.open');
  broker.acceptExtensionEvents(replacement.connection_id, [{
    kind: 'response', command_id: open.command_id, result: { tab_id: '9' },
  }]);
  assert.equal((await queued).tab_id, '9');
});

test('stale online metadata is not command-dispatch authority', async () => {
  let clock = 1_000;
  const broker = new BrokerState({ now: () => clock });
  const session = broker.createSession().agent_session_id;
  const connected = broker.connectExtension({ browser_instance_id: 'browser-1' });
  assert.equal((await broker.rpc(session, 'system.capabilities')).extension_connected, false);
  assert.equal(broker.doctor().extension_connections[0].dispatch_state, 'consumer_not_started');

  const firstPoll = broker.pollCommands(connected.connection_id, 5);
  assert.equal((await broker.rpc(session, 'system.capabilities')).extension_connected, true);
  await firstPoll;

  clock += 1_001;
  const capabilities = await broker.rpc(session, 'system.capabilities');
  assert.equal(capabilities.extension_connected, false);
  assert.deepEqual(capabilities.connected_extensions, []);
  assert.throws(
    () => broker.enqueueCommand(session, 'tabs.open', {}, 1_000, { browserInstanceId: 'browser-1' }),
    (error) => error.code === 'EXTENSION_OFFLINE' && error.retry_safe === true,
  );
  assert.equal(broker.doctor().online_extension_connections, 1);
  assert.equal(broker.doctor().dispatchable_extension_connections, 0);
  assert.equal(broker.doctor().stale_extension_connections, 1);
  assert.equal(broker.doctor().extension_connections[0].dispatch_state, 'consumer_stale');
});

test('a command queued in the poll transition is rejected when the consumer never returns', async () => {
  let clock = 2_000;
  const broker = new BrokerState({ now: () => clock });
  const session = broker.createSession().agent_session_id;
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  broker.connections.get(connected.connection_id).last_poll_at = clock - 999;

  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1_000, {
    browserInstanceId: 'browser-1',
  });
  clock += 2;
  await assert.rejects(pending, (error) => error.code === 'EXTENSION_OFFLINE'
    && error.details.stage === 'extension_queue'
    && error.retry_safe === true);
  assert.equal(broker.commands.values().next().value.state, 'failed');
  assert.deepEqual(broker.connections.get(connected.connection_id).queue, []);
});

test('Extension loss rejects queued work immediately when no reconnect is pending', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1000);
  broker.disconnectExtension(connection.connection_id, 'power_loss');
  await assert.rejects(pending, (error) => (
    error.code === 'EXTENSION_OFFLINE' && error.retry_safe === true
  ));
});

test('a renewed consumer cannot claim another browser instance queue', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const other = connectTestConsumer(broker, { browser_instance_id: 'browser-2' });
  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1000, {
    browserInstanceId: 'browser-1',
  });

  assert.deepEqual(await broker.pollCommands(other.connection_id, 5), []);
  const renewed = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  const [command] = await broker.pollCommands(renewed.connection_id, 10);
  assert.equal(command.kind, 'tabs.open');
  broker.acceptExtensionEvents(renewed.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { tab_id: '8' },
  }]);
  assert.equal((await pending).tab_id, '8');
});

test('replacement diagnostics identify browser-family consumer contention', () => {
  const broker = new BrokerState({ now: () => 1_000 });
  connectTestConsumer(broker, {
    browser_instance_id: 'browser-1', browser_family: 'chrome',
    browser_session_id: 'session-1', worker_instance_id: 'worker-1',
  });
  connectTestConsumer(broker, {
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
  connectTestConsumer(broker, {
    browser_instance_id: 'browser-1', browser_family: 'chrome',
    extension_candidate: candidate,
  });
  broker.leaseTab('7', session, { browser_instance_id: 'browser-1', ownership: 'agent' });

  const capabilities = await broker.rpc(session, 'system.capabilities');
  assert.equal(capabilities.schema, 'saccade.capabilities/8');
  assert.equal(capabilities.browser_family, 'chrome');
  assert.deepEqual(capabilities.extension_candidate, candidate);
  assert.deepEqual(capabilities.connected_extensions, [{
    browser_instance_id: 'browser-1', browser_family: 'chrome', extension_candidate: candidate, media_version: 0, visual_version: 0, scene_version: 0, scene_access_version: 0, connection_request_version: 0, captions_version: 0, media_consent_version: 0, visual_consent_version: 0, consent_wait_version: 0, session_consent_version: 0,
  }]);
  assert.equal(capabilities.leased_tabs[0].browser_family, 'chrome');
  assert.deepEqual(capabilities.leased_tabs[0].extension_candidate, candidate);
});

function mediaFixture(version = 1) {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1', media_version: version });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1', ownership: 'agent' });
  const full = observation();
  full.objects = [{ object_id: 'v1', kind: 'opaque_video', media: { media_id: 'm1', duration_s: 20 } }];
  broker.acceptTruth('observation', full);
  const args = { tab_id: '7', document_id: 'document-1', object_id: 'v1', media_id: 'm1', mode: 'overview' };
  const result = { schema: 'saccade.media/1', ...args, frames: [{ time_s: 1, requested_time_s: 1,
    method: 'decoded_frame', image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' } }] };
  return { broker, owner, connection, args, result };
}

test('media requires current session/document/version and advertises compatibility', async () => {
  const { broker, owner, args } = mediaFixture(0);
  await assert.rejects(broker.rpc(owner, 'media.read', args), { code: 'MEDIA_EXTENSION_UPGRADE_REQUIRED' });
  await assert.rejects(broker.rpc(owner, 'media.read', { ...args, media_id: 'old' }), { code: 'MEDIA_STALE' });
  await assert.rejects(broker.rpc(owner, 'media.read', { ...args, document_id: 'old' }), { code: 'MEDIA_STALE' });
  const other = broker.createSession().agent_session_id;
  await assert.rejects(broker.rpc(other, 'media.read', args), /another Agent/);
  const caps = await broker.rpc(owner, 'system.capabilities');
  assert.equal(caps.media_observation.version, 1);
  assert.equal(caps.connected_extensions[0].media_version, 0);
});

test('media rejects an expired inherited deadline without enqueueing', async () => {
  const { broker, owner, args } = mediaFixture();
  await assert.rejects(broker.rpc(owner, 'media.read', args, 30000, 99, Date.now() - 1), { code: 'DEADLINE_EXCEEDED' });
  assert.equal(broker.commands.size, 0);
});

test('media consent invalidation removes only the corresponding browser tab history', () => {
  const { broker, owner, connection } = mediaFixture();
  const key = JSON.stringify([owner, '7', 'document-1', 'm1']);
  broker.retainMediaHistory(key, { at: Date.now(), details: 1, times: [1] });
  const other = connectTestConsumer(broker, { browser_instance_id: 'other-browser', media_version: 1 });
  broker.acceptExtensionEvents(other.connection_id, [{ kind: 'media.invalidated', tab_id: '7' }]);
  assert.equal(broker.mediaReads.size, 1);
  broker.acceptExtensionEvents(connection.connection_id, [{ kind: 'media.invalidated', tab_id: '7' }]);
  assert.equal(broker.mediaReads.size, 0);
  assert.equal(broker.mediaExpiry.size, 0);
});

test('disconnecting the owning Agent cancels delivered media and clears observation history', async () => {
  const { broker, owner, args, connection } = mediaFixture();
  const sent = [];
  broker.connections.get(connection.connection_id).keepalive_socket = { send: (value) => sent.push(JSON.parse(value)) };
  broker.retainMediaHistory(JSON.stringify([owner, '7', 'document-1', 'm1']), { at: Date.now(), details: 1, times: [1] });
  const pending = broker.rpc(owner, 'media.read', args, 50, 'close-test');
  const rejected = assert.rejects(pending);
  await broker.pollCommands(connection.connection_id);
  broker.closeSession(owner);
  assert.equal(sent[0].kind, 'command.cancel');
  assert.equal(broker.mediaReads.size, 0);
  assert.equal(broker.mediaExpiry.size, 0);
  await rejected;
});

test('media catalog preserves post provenance without exposing signed addresses', async () => {
  const { broker, owner, connection } = mediaFixture();
  const args = { tab_id: '7', document_id: 'document-1', mode: 'catalog' };
  const value = await deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), {
    schema: 'saccade.media/1', ...args, videos: [{ object_id: 'v1', media_id: 'm1', duration_s: 20,
      source: { post_url: 'https://x.com/a/status/123?secret=1', author_claim: 'Created automatically', raw_url: 'secret' } }],
  });
  assert.equal(value.videos[0].source.post_url, 'https://x.com/a/status/123');
  assert.equal(value.videos[0].source.author_claim, 'Created automatically');
  assert.equal(JSON.stringify(value).includes('secret'), false);
  assert.equal(value.complete, false);
});

test('media bounds frames, timestamps and two detail requests; only observed single frames allowed', async () => {
  const { broker, owner, connection, args, result } = mediaFixture();
  await assert.rejects(broker.rpc(owner, 'media.read', { ...args, mode: 'detail', time_s: 1 }), { code: 'MEDIA_TIME_NOT_OBSERVED' });
  const observed = await deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), { ...result, raw_url: 'secret' });
  assert.equal(observed.frames.length, 1);
  assert.equal(observed.raw_url, undefined);
  assert.equal(JSON.stringify([...broker.mediaReads.values()]).includes('YQ=='), false);
  for (let i = 0; i < 2; i++) await deliver(broker, connection.connection_id,
    broker.rpc(owner, 'media.read', { ...args, mode: 'detail', time_s: 1 }, 1000), result);
  await assert.rejects(broker.rpc(owner, 'media.read', { ...args, mode: 'detail', start_s: 0, end_s: 2 }), { code: 'MEDIA_DETAIL_LIMIT' });
  await assert.rejects(deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), {
    ...result, frames: Array(9).fill(result.frames[0]),
  }), { code: 'MEDIA_INVALID_RESPONSE' });
  await assert.rejects(deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), {
    ...result, frames: [{ ...result.frames[0], time_s: -1 }],
  }), { code: 'MEDIA_INVALID_RESPONSE' });
  await assert.rejects(deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), {
    ...result, frames: [{ ...result.frames[0], image: { type: 'image', mimeType: 'image/webp', data: Buffer.alloc(4 * 1024 * 1024 + 1).toString('base64') } }],
  }), { code: 'MEDIA_INVALID_RESPONSE' });
});

test('one-hour media accepts late timestamps but keeps source, frame and detail bounds', async () => {
  const { broker, owner, connection, args, result } = mediaFixture();
  const media = broker.truth.get('7').full.objects[0].media;
  media.duration_s = 3600;
  const late = { ...result, frames: [{ ...result.frames[0], time_s: 3599.75, requested_time_s: 3599.75 }] };
  const overview = await deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000), late);
  assert.equal(overview.frames[0].time_s, 3599.75);
  await deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', { ...args, mode: 'detail', start_s: 3597, end_s: 3600 }, 1000), late);
  await assert.rejects(broker.rpc(owner, 'media.read', { ...args, mode: 'detail', start_s: 3596, end_s: 3600 }), { code: 'INVALID_REQUEST' });
  for (const field of ['time_s', 'requested_time_s']) await assert.rejects(deliver(broker, connection.connection_id, broker.rpc(owner, 'media.read', args, 1000),
    { ...late, frames: [{ ...late.frames[0], [field]: 3600.01 }] }), { code: 'MEDIA_INVALID_RESPONSE' });
  media.duration_s = 3600.01;
  await assert.rejects(broker.rpc(owner, 'media.read', args), { code: 'MEDIA_DURATION_LIMIT' });
  assert.equal((await broker.rpc(owner, 'system.capabilities')).media_observation.max_duration_s, 3600);
  broker.closeSession(owner);
});

test('cancellation revokes completed authorization commands without claiming confirmed stop', async () => {
  const { broker, owner, connection } = mediaFixture();
  const signals = [];
  broker.connections.get(connection.connection_id).keepalive_socket = { send: value => signals.push(JSON.parse(value)) };
  const pending = broker.enqueueCommand(owner, 'media.authorization.accept', {}, 1000, { clientRequestId: 'auth-req' });
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  broker.acceptExtensionEvents(connection.connection_id, [{ kind: 'response', command_id: command.command_id, result: { granted: true } }]);
  await pending;
  assert.equal(broker.cancelRequest(owner, 'auth-req').cancelled, false);
  assert.equal(signals[0].command_id, command.command_id);
});

test('delivered media cancellation and deadline signal Extension; late pixels are discarded', async () => {
  const { broker, owner, connection, args, result } = mediaFixture();
  const signals = [];
  broker.connections.get(connection.connection_id).keepalive_socket = { send: (value) => signals.push(JSON.parse(value)) };
  const pending = broker.rpc(owner, 'media.read', args, 1000, 'req1');
  const [command] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(broker.cancelRequest(owner, 'req1').cancelled, false);
  assert.equal(signals[0].kind, 'command.cancel');
  assert.equal(signals[0].command_id, command.command_id);
  assert.equal(signals[0].broker_epoch, broker.epoch);
  broker.acceptExtensionEvents(connection.connection_id, [{ kind: 'response', command_id: command.command_id, result }]);
  await assert.rejects(pending, { code: 'CANCELLED' });
  const timeout = broker.rpc(owner, 'media.read', args, 10);
  await broker.pollCommands(connection.connection_id, 10);
  await assert.rejects(timeout, { code: 'OUTCOME_UNKNOWN' });
  assert.equal(signals.length, 2);
});

test('Extension handshake rejects unbounded or unrecognized candidate metadata', () => {
  const broker = new BrokerState();
  assert.throws(() => connectTestConsumer(broker, {
    browser_instance_id: 'browser-1', browser_family: 'safari',
    extension_candidate: { schema: 'saccade.extension-candidate/1', id: 'a'.repeat(64), version: '0.4.0' },
  }), /browser_family is invalid/);
  assert.throws(() => connectTestConsumer(broker, {
    browser_instance_id: 'browser-1', browser_family: 'edge',
    extension_candidate: { schema: 'saccade.extension-candidate/1', id: 'not-a-digest', version: '0.4.0' },
  }), /extension_candidate is invalid/);
});

test('empty long-poll completion starts a bounded consumer handoff grace', async () => {
  let clock = 1_000;
  const broker = new BrokerState({ now: () => clock });
  const session = broker.createSession().agent_session_id;
  const connection = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const poll = broker.pollCommands(connection.connection_id, 2);
  clock += EXTENSION_POLL_HEARTBEAT_MS;
  assert.ok(broker.activeConnection('browser-1'));
  assert.deepEqual(await poll, []);
  assert.ok(broker.activeConnection('browser-1'));
  const pending = broker.enqueueCommand(session, 'tabs.open', {}, 1_000);
  const [command] = await broker.pollCommands(connection.connection_id, 2);
  broker.acceptExtensionEvents(connection.connection_id, [{
    kind: 'response', command_id: command.command_id, result: { tab_id: '7' },
  }]);
  assert.equal((await pending).tab_id, '7');
  assert.deepEqual(await broker.pollCommands(connection.connection_id, 2), []);
  clock += 1_001;
  assert.equal(broker.activeConnection('browser-1'), undefined);
});

test('disconnecting a pending empty poll does not renew consumer authority', async () => {
  let clock = 1_000;
  const broker = new BrokerState({ now: () => clock });
  const connected = broker.connectExtension({ browser_instance_id: 'browser-1' });
  const poll = broker.pollCommands(connected.connection_id, 20);
  clock += EXTENSION_POLL_HEARTBEAT_MS;
  broker.disconnectExtension(connected.connection_id, 'power_loss');
  assert.deepEqual(await poll, []);
  assert.equal(broker.connections.get(connected.connection_id).last_poll_at, 1_000);
  assert.equal(broker.activeConnection('browser-1'), undefined);
});

test('an expired long-poll waiter cannot swallow the next command', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(firstBroker, { browser_instance_id: 'browser-1' });
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
  const replacement = connectTestConsumer(restarted, { browser_instance_id: 'browser-1' });
  assert.deepEqual(await restarted.pollCommands(replacement.connection_id, 5), []);

  firstBroker.disconnectExtension(connection.connection_id, 'test_end');
  await assert.rejects(pending, (error) => error.code === 'OUTCOME_UNKNOWN');
});

test('state write failure never acknowledges a delivered command or leaves a new lease active', async () => {
  const broker = new BrokerState();
  const session = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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

test('tab-sharing HTTP route requires Extension origin and matching connection proof', async (context) => {
  const broker = new BrokerState();
  const runtime = createBrokerServer(broker, { port: 0 });
  try { await runtime.listen(); } catch (error) {
    if (error.code === 'EPERM') return context.skip('sandbox forbids loopback listen');
    throw error;
  }
  context.after(() => new Promise((resolve) => runtime.server.close(resolve)));
  const base = `http://127.0.0.1:${runtime.server.address().port}`;
  const origin = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop';
  const post = async (route, body, suppliedOrigin) => fetch(`${base}/v1/extension/${route}`, {
    method: 'POST', headers: { 'content-type': 'application/json', ...(suppliedOrigin ? { origin: suppliedOrigin } : {}) },
    body: JSON.stringify(body),
  });
  const connected = await (await post('connect', { browser_instance_id: 'browser-1' }, origin)).json();
  const body = { connection_id: connected.connection_id, operation: 'status', tab_id: '7' };
  assert.equal((await post('tab-sharing', body)).status, 403);
  const wrong = await (await post('tab-sharing', body, 'chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb')).json();
  assert.equal(wrong.error.code, 'EXTENSION_AUTH_FAILED');
  const valid = await (await post('tab-sharing', body, origin)).json();
  assert.equal(valid.state, 'unassigned');
  assert.equal(valid.tab_sharing_version, 1);
});

test('Extension WebSocket heartbeat is origin-bound and value-free', async (context) => {
  const broker = new BrokerState();
  const connected = connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
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
  const closeDeadline = Date.now() + 250;
  while (broker.connections.get(connected.connection_id).state === 'online'
      && Date.now() < closeDeadline) {
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.equal(broker.connections.get(connected.connection_id).state, 'offline');
  assert.equal(broker.doctor().extension_connected, false);
});
