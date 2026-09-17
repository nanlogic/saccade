'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { BrokerState } = require('../src/broker');

function setup() {
  let time = 1000;
  const broker = new BrokerState({ now: () => time });
  const origin = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop';
  const attach = (browser = 'browser-1') => {
    const connection = broker.connectExtension({ browser_instance_id: browser, connection_request_version: 1 }, origin);
    broker.connections.get(connection.connection_id).last_poll_at = time;
    return connection;
  };
  const connection = attach();
  const a = broker.createSession().agent_session_id;
  const b = broker.createSession().agent_session_id;
  const status = (extra = {}) => broker.extensionTabSharing({ connection_id: connection.connection_id, tab_id: '7', operation: 'status', ...extra }, origin);
  const request = (session = a, label = 'Mixamo animation download', id = 1) => broker.rpc(session, 'tabs.open',
    { claim: 'request', connection_label: label, browser_instance_id: 'browser-1' }, 10000, id);
  return { broker, origin, attach, connection, a, b, status, request, advance: ms => { time += ms; } };
}

test('two task invitations reveal no tabs and approval consumes only the exact request', async () => {
  const s = setup();
  const first = await s.request();
  const second = await s.request(s.b, 'Another task');
  assert.equal(s.status().requests.length, 2);
  assert.equal(s.broker.leases.size, 0);
  assert.equal(s.broker.commands.size, 0);
  assert.deepEqual(s.broker.listTabs(s.a), []);
  assert.match(first.pairing_code, /^[A-F0-9]{6}$/);
  assert.equal(s.status().requests[0].label, 'Mixamo animation download');
  assert.doesNotMatch(JSON.stringify(s.status()), /resume_token|upload_roots|document_url|client_request_id/);
  assert.throws(() => s.status({ operation: 'assign', agent_session_id: s.a, request_id: second.request_id }), { code: 'SHARING_REQUEST_EXPIRED' });
  const assigned = s.status({ operation: 'assign', agent_session_id: s.a, request_id: first.request_id });
  assert.equal(assigned.agent_session_id, s.a);
  assert.equal(assigned.requests.length, 1);
  assert.equal(s.broker.listTabs(s.a)[0].ownership, 'user_shared');
  assert.deepEqual(s.broker.listTabs(s.b), []);
  assert.throws(() => s.status({ operation: 'assign', agent_session_id: s.b, request_id: second.request_id }), { code: 'TAB_ALREADY_LEASED' });
  assert.throws(() => s.status({ operation: 'assign', agent_session_id: s.a, request_id: first.request_id }), { code: 'SHARING_REQUEST_EXPIRED' });
});

test('expired, replaced, cancelled, disconnected and cross-browser invitations cannot grant access', async () => {
  const s = setup();
  const old = await s.request();
  const fresh = await s.request();
  assert.equal(s.status().requests.length, 1);
  const approve = invitation => s.status({ operation: 'assign', agent_session_id: s.a, request_id: invitation.request_id });
  assert.throws(() => approve(old), { code: 'SHARING_REQUEST_EXPIRED' });
  const other = s.attach('browser-2');
  assert.throws(() => s.broker.extensionTabSharing({ connection_id: other.connection_id, tab_id: '8', operation: 'assign',
    agent_session_id: s.a, request_id: fresh.request_id }, s.origin), { code: 'SHARING_REQUEST_EXPIRED' });
  s.broker.cancelRequest(s.a, 1);
  assert.throws(() => approve(fresh), { code: 'SHARING_REQUEST_EXPIRED' });
  s.broker.cancelRequest(s.a, 99);
  await assert.rejects(s.request(s.a, 'Cancelled before creation', 99), { code: 'CANCELLED' });
  const expired = await s.request(s.a, 'Expires', 2);
  s.advance(120001);
  assert.throws(() => approve(expired), { code: 'SHARING_REQUEST_EXPIRED' });
  s.broker.connections.get(s.connection.connection_id).last_poll_at = s.broker.now();
  const reconnect = await s.request(s.a, 'Reconnect', 3);
  s.broker.disconnectExtension(s.connection.connection_id);
  const next = s.attach();
  assert.throws(() => s.broker.extensionTabSharing({ connection_id: next.connection_id, tab_id: '8', operation: 'assign',
    agent_session_id: s.a, request_id: reconnect.request_id }, s.origin), { code: 'SHARING_REQUEST_EXPIRED' });
  assert.equal(s.broker.leases.size, 0);
});

test('inactive sessions leave the picker without closing, orphaning or transferring their tabs', () => {
  const s = setup();
  s.broker.leaseTab('7', s.a, { browser_instance_id: 'browser-1', ownership: 'user_shared' });
  s.advance(300001);
  assert.equal(s.status().sessions.length, 0);
  assert.equal(s.status().state, 'active');
  assert.equal(s.broker.sessions.get(s.a).state, 'online');
  assert.equal(s.broker.leases.get('7').agent_session_id, s.a);
  assert.throws(() => s.status({ tab_id: '8', operation: 'assign', agent_session_id: s.b }), { code: 'SESSION_INACTIVE' });
  s.broker.touchSession(s.b);
  assert.deepEqual(s.status().sessions.map(item => item.agent_session_id), [s.b]);
  s.broker.closeSession(s.a);
  assert.equal(s.status().state, 'orphaned');
  assert.throws(() => s.status({ operation: 'assign', agent_session_id: s.b }), { code: 'TAB_ALREADY_LEASED' });
});

test('closed sessions, restart and invalid labels do not leave reusable invitations', async () => {
  const s = setup();
  const invitation = await s.request();
  s.broker.closeSession(s.a);
  assert.equal(s.status().requests.length, 0);
  assert.throws(() => s.status({ operation: 'assign', agent_session_id: s.a, request_id: invitation.request_id }), { code: 'SESSION_OFFLINE' });
  assert.equal(new BrokerState().sharingRequests.size, 0);
  for (const connection_label of ['', 'x'.repeat(81), 'hidden\nlabel', 'direction\u202e']) {
    await assert.rejects(s.broker.rpc(s.b, 'tabs.open', { claim: 'request', connection_label }), { code: 'INVALID_REQUEST' });
  }
  await assert.rejects(s.broker.rpc(s.b, 'tabs.open', { claim: 'request', connection_label: 'Task', tab_id: '7' }), { code: 'INVALID_REQUEST' });
  s.broker.connections.get(s.connection.connection_id).connection_request_version = 0;
  await assert.rejects(s.request(s.b, 'Old Extension', 5), { code: 'SHARING_UPGRADE_REQUIRED' });
});
