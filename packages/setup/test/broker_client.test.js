'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const { rpc } = require('../src/broker_client');

function response(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status, headers: { 'content-type': 'application/json' },
  });
}

test('media forwards one absolute deadline and never reconnects or replays after transport loss', async (context) => {
  const oldFetch = global.fetch; context.after(() => { global.fetch = oldFetch; });
  const routes = []; let body;
  const started = Date.now();
  global.fetch = async (url, options) => {
    routes.push(new URL(url).pathname);
    if (routes.at(-1) === '/v1/cancel') return response({ ok: true });
    body = JSON.parse(options.body);
    throw new DOMException('Timed out', 'TimeoutError');
  };
  await assert.rejects(rpc({ agent_session_id: 'agent', resume_token: 'proof' }, 'media.read', {}, 500, 42),
    { code: 'OUTCOME_UNKNOWN', retry_safe: false });
  assert(body.deadline_at >= started + 500 && body.deadline_at <= Date.now() + 500);
  assert.deepEqual(routes, ['/v1/rpc', '/v1/cancel']);
});

test('a live MCP adapter resumes its exact session after Broker process replacement', async (context) => {
  const originalFetch = global.fetch;
  const session = {
    agent_session_id: 'agent_original',
    broker_epoch: 'broker_old',
    resume_token: 'resume_in-memory-proof',
  };
  let rpcCalls = 0;
  let resumeCalls = 0;
  global.fetch = async (url, options = {}) => {
    const route = new URL(url).pathname;
    if (route === '/v1/health') {
      return response({ schema: 'saccade.node-broker/1', broker_epoch: 'broker_new' });
    }
    if (route === '/v1/sessions' && options.method === 'POST') {
      resumeCalls += 1;
      assert.deepEqual(JSON.parse(options.body), {
        resume_token: 'resume_in-memory-proof', upload_root: process.cwd(),
      });
      return response({
        agent_session_id: 'agent_original', broker_epoch: 'broker_new',
        resume_token: 'resume_rotated-proof', resumed: true, resumed_tabs: 1,
      });
    }
    if (route === '/v1/rpc') {
      rpcCalls += 1;
      if (rpcCalls === 1) {
        return response({ ok: false, error: { code: 'SESSION_OFFLINE', message: 'restart' } }, 400);
      }
      return response({ ok: true, result: {
        agent_session_id: 'agent_original', broker_epoch: 'broker_new',
        leased_tabs: [{ tab_id: '7', readiness: 'awaiting_truth' }],
      } });
    }
    throw new Error(`unexpected route ${options.method || 'GET'} ${route}`);
  };
  context.after(() => { global.fetch = originalFetch; });

  const capabilities = await rpc(session, 'system.capabilities', {}, 1000, 1);
  assert.equal(rpcCalls, 2);
  assert.equal(resumeCalls, 1);
  assert.equal(capabilities.agent_session_id, session.agent_session_id);
  assert.equal(session.broker_epoch, 'broker_new');
  assert.equal(session.resume_token, 'resume_rotated-proof');
  assert.deepEqual(capabilities.leased_tabs, [{ tab_id: '7', readiness: 'awaiting_truth' }]);
});

test('visual authorization preserves the inherited deadline and never replays on transport failure', async (context) => {
  const oldFetch = global.fetch; context.after(() => { global.fetch = oldFetch; });
  for (const method of ['visual.authorization.accept', 'visual.read']) {
    const routes = []; const deadline = Date.now() + 1000;
    global.fetch = async (url, options) => {
      routes.push(new URL(url).pathname);
      if (routes.at(-1) === '/v1/cancel') return response({ ok: true });
      assert.equal(JSON.parse(options.body).deadline_at, deadline);
      throw new DOMException('Timed out', 'TimeoutError');
    };
    await assert.rejects(rpc({ agent_session_id: 'agent', resume_token: 'proof' }, method, {}, 5000, 44, deadline), { code: 'OUTCOME_UNKNOWN', retry_safe: false });
    assert.deepEqual(routes, ['/v1/rpc', '/v1/cancel']);
  }
});

test('visual authorization transport preserves human wait budget and caps snapshot execution without renewing it', async (context) => {
  const oldFetch = global.fetch;
  const oldNow = Date.now;
  let now = oldNow();
  const started = now;
  const deadline = now + 30000;
  const calls = [];
  context.after(() => { global.fetch = oldFetch; Date.now = oldNow; });
  Date.now = () => now;
  global.fetch = async (url, options) => {
    assert.equal(new URL(url).pathname, '/v1/rpc');
    calls.push(JSON.parse(options.body));
    return response({ ok: true, result: {} });
  };
  const session = { agent_session_id: 'agent', resume_token: 'proof' };
  await rpc(session, 'visual.authorization.prepare', {}, 30000, 45, deadline);
  now = started + 15000;
  await rpc(session, 'visual.authorization.accept', {}, 30000, 45, deadline);
  await rpc(session, 'visual.read', { timeout_ms: 30000 }, 30000, 45, deadline);
  await rpc(session, 'visual.read', { mode: 'sequence', timeout_ms: 30000 }, 30000, 46, deadline);
  now = started + 28000;
  await rpc(session, 'visual.read', { timeout_ms: 30000 }, 30000, 47, deadline);
  assert.deepEqual(calls.map(call => call.deadline_at), [deadline, deadline, started + 25000, deadline, deadline]);
});
