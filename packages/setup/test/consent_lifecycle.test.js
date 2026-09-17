'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createMediaAuthorization } = require('../src/media_authorization');
const tick = () => new Promise(setImmediate);
function harness(kind = 'visual') {
  let now = 1000000, granted = false, stale = false;
  const messages = [], calls = [];
  const api = createMediaAuthorization({ observation: kind, now: () => now, isCancelled: () => false,
    output: { write: line => messages.push(JSON.parse(line)) },
    invoke: async (method, args, timeout, id, deadline) => {
      calls.push({ method, args, timeout, id, deadline });
      if (stale) throw Object.assign(new Error('source changed'), { code: 'STALE' });
      if (method.endsWith('prepare')) return granted ? { session_consent_version: 2, granted: true } : {
        session_consent_version: 2, granted: false, challenge: 'unchanged-challenge', grant_ttl_ms: args.grant_ttl_ms, grant_scope: args.grant_scope, grant_expires_at: args.grant_expires_at, consent_wait_version: 1, consent_expires_at: now + 600000,
      };
      granted = true; return { session_consent_version: 2, granted: true, grant_scope: 'session_observation', expires_at: now + 10800000 };
    },
  });
  api.initialize({ elicitation: { form: {} } });
  const request = id => ({ id, params: { arguments: { tab_id: 'tab', document_id: 'doc' } } });
  const answer = () => api.receive({ id: messages[0].id, result: { action: 'accept', content: { allow: true } } });
  return { api, calls, messages, request, answer, advance: ms => { now += ms; }, now: () => now, stale: () => { stale = true; }, restore: () => { stale = false; } };
}
for (const kind of ['visual', 'media']) {
  test(`${kind}: a six-minute human wait starts acquisition only after the answer and revalidates once`, async t => {
    t.mock.timers.enable({ apis: ['setTimeout'] });
    const h = harness(kind); t.after(() => h.api.close());
    const pending = h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000 });
    await tick(); h.advance(360000); t.mock.timers.tick(360000); await tick();
    assert.equal(h.calls.length, 1); assert.equal(h.messages.length, 1);
    h.answer(); const result = await pending;
    assert.equal(result.deadline_at, h.now() + 30000);
    assert.equal(h.calls[1].deadline, result.deadline_at);
    assert.equal(h.calls[1].method, `${kind}.authorization.prepare`);
    assert.equal(h.calls[2].args.challenge, 'unchanged-challenge');
    assert.equal(h.calls[2].deadline, result.deadline_at);
    assert.equal(h.calls.length, 3);
    await h.api.ensure(h.request(2), h.now() + 30000, { executionTimeoutMs: 30000 });
    assert.equal(h.messages.length, 1);
  });
}
test('a day-long human wait keeps one prompt and refreshes only the expired document challenge', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const h = harness();
  const result = h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000 });
  await tick(); h.advance(86400000); t.mock.timers.tick(86400000); await tick();
  assert.equal(h.messages.length, 1); assert.equal(h.calls.length, 1);
  h.answer(); const accepted = await result;
  assert.equal(accepted.deadline_at, h.now() + 30000);
  assert.deepEqual(h.calls.map(c => c.method), ['visual.authorization.prepare', 'visual.authorization.prepare', 'visual.authorization.accept']);
  assert.equal(h.calls[1].deadline, h.calls[2].deadline);
  assert.equal(h.messages.length, 1); h.api.close();
});
test('deadline is checked when an answer arrives even if the timer callback is delayed', async () => {
  const h = harness();
  const result = assert.rejects(h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000, gateDeadline: h.now() + 600000 }), { code: 'VISUAL_AUTHORIZATION_TIMEOUT' });
  await tick(); h.advance(600001); h.answer(); await result;
  assert.equal(h.calls.length, 1); assert.equal(h.messages[1].method, 'notifications/cancelled');
});
test('parent cancellation cancels its prompt and prevents delayed acceptance', async () => {
  const h = harness();
  const result = assert.rejects(h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000 }), { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
  await tick(); h.api.cancel(1); await result;
  assert.equal(h.messages[1].params.requestId, h.messages[0].id);
  assert.equal(h.answer(), false); assert.equal(h.calls.length, 1);
});
test('source changes while waiting reject exact-document revalidation before acceptance', async () => {
  const h = harness();
  const result = assert.rejects(h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000 }), { code: 'STALE' });
  await tick(); h.stale(); h.answer(); await result;
  assert.equal(h.calls.filter(c => c.method.endsWith('prepare')).length, 2);
  assert.equal(h.calls.filter(c => c.method.endsWith('accept')).length, 0);
  assert(h.calls.every(c => c.args.document_id === 'doc'));
  assert.equal(h.api.describe().consent_active, true);
  await assert.rejects(h.api.ensure(h.request(2), h.now() + 30000, { executionTimeoutMs: 30000 }), { code: 'STALE' });
  assert.equal(h.messages.length, 1);
});
test('concurrent phased reads share the prompt, not an elapsed execution deadline', async () => {
  const h = harness();
  const results = Promise.all([1, 2, 3].map(id => h.api.ensure(h.request(id), h.now() + 1000, { executionTimeoutMs: 1000 })));
  await tick(); h.advance(40000); h.answer();
  for (const result of await results) assert.equal(result.deadline_at, h.now() + 1000);
  assert.equal(h.messages.length, 1);
  assert.equal(h.calls.filter(c => c.method.endsWith('accept')).length, 1);
});

test('expired challenge and changed document stop the old read without losing consent for a new tab', async t => {
  const h = harness(); t.after(() => h.api.close());
  const rejected = assert.rejects(h.api.ensure(h.request(1), h.now() + 30000, { executionTimeoutMs: 30000 }), { code: 'STALE' });
  await tick(); h.advance(86400000); h.stale(); h.answer(); await rejected;
  assert.deepEqual(h.calls.map(c => c.method), ['visual.authorization.prepare', 'visual.authorization.prepare']);
  assert(h.calls.every(c => c.args.document_id === 'doc'));
  assert.equal(h.api.describe().consent_active, true);
  h.restore();
  await h.api.ensure({ id: 2, params: { arguments: { tab_id: 'new-tab', document_id: 'new-doc' } } }, h.now() + 30000, { executionTimeoutMs: 30000 });
  assert.equal(h.calls.at(-1).args.source, 'session_confirmation');
  assert.equal(h.calls.at(-1).args.document_id, 'new-doc');
  assert.equal(h.messages.length, 1);
});
