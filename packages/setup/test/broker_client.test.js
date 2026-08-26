'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const { rpc } = require('../src/broker_client');

function response(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status, headers: { 'content-type': 'application/json' },
  });
}

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
      assert.deepEqual(JSON.parse(options.body), { resume_token: 'resume_in-memory-proof' });
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
