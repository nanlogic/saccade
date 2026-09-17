'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createMediaAuthorization } = require('../src/media_authorization');
const tick = () => new Promise(setImmediate);
function harness() {
  const messages = [], calls = [], grants = new Set(), revoked = new Set();
  const api = createMediaAuthorization({ observation: 'visual', isCancelled: () => false,
    output: { write: line => messages.push(JSON.parse(line)) },
    invoke: async (method, args, timeout, id, deadline) => {
      calls.push({ method, args, id, deadline });
      const key = JSON.stringify([args.tab_id, args.document_id]);
      if (revoked.has(args.tab_id)) throw Object.assign(new Error('Off'), { code: 'VISUAL_ACCESS_REVOKED' });
      if (method.endsWith('prepare')) return grants.has(key) ? { session_consent_version: 2, granted: true } : { session_consent_version: 2, granted: false, challenge: 'test-challenge', grant_ttl_ms: args.grant_ttl_ms, grant_scope: args.grant_scope, grant_expires_at: args.grant_expires_at };
      const prepared = calls.findLast(call=>call.id===id && call.method.endsWith('prepare')).args;
      grants.add(key); return { session_consent_version: 2, granted: true, grant_scope: prepared.grant_scope, expires_at: prepared.grant_expires_at ?? Date.now()+10800000 };
    },
  });
  api.initialize({ elicitation: { form: {} } });
  const request = (id, tab = 'tab', doc = 'doc') => ({ id, params: { arguments: { tab_id: tab, document_id: doc } } });
  const answer = (index = 0, action = 'accept') => api.receive({ id: messages[index].id, result: { action, content: { allow: true } } });
  return { api, request, answer, messages, calls, grants, revoked };
}

test('seven concurrent reads use one prompt and each follower rechecks current grant', async t => {
  const h = harness(); t.after(() => h.api.close());
  const deadline = Date.now() + 2000;
  const tasks = Array.from({ length: 7 }, (_, id) => h.api.ensure(h.request(id), deadline));
  await tick(); assert.equal(h.messages.length, 1); h.answer(); await Promise.all(tasks);
  assert.equal(h.calls.filter(c => c.method.endsWith('accept')).length, 1);
  assert.equal(h.calls.filter(c => c.method.endsWith('prepare')).length, 7);
  assert(h.calls.every(c => c.deadline === deadline));
  await h.api.ensure(h.request(9), deadline);
  assert.equal(h.messages.length, 1);
});

test('one declined prompt rejects every follower without retry or grant', async t => {
  const h = harness(); t.after(() => h.api.close());
  const tasks = Array.from({ length: 7 }, (_, id) => h.api.ensure(h.request(id), Date.now() + 1000));
  const settled = Promise.allSettled(tasks);
  await tick(); h.answer(0, 'decline');
  for (const r of await settled) { assert.equal(r.status, 'rejected'); assert.equal(r.reason.code, 'VISUAL_CLIENT_CONFIRMATION_DECLINED'); }
  assert.equal(h.messages.length, 1); assert.equal(h.calls.length, 1);
});

test('cancelling a follower does not cancel the original user confirmation', async t => {
  const h = harness(); t.after(() => h.api.close());
  const leader = h.api.ensure(h.request(1), Date.now() + 2000);
  const follower = assert.rejects(h.api.ensure(h.request(2), Date.now() + 1000), { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
  await tick(); h.api.cancel(2); await follower;
  h.answer(); await leader; assert.equal(h.messages.length, 1);
  assert.equal(h.calls.some(c => c.id === 2), false);
});

test('follower timeout cannot extend its deadline or cancel another request', async t => {
  const h = harness(); t.after(() => h.api.close());
  const leader = h.api.ensure(h.request(1), Date.now() + 2000);
  await assert.rejects(h.api.ensure(h.request(2), Date.now() + 15), { code: 'VISUAL_AUTHORIZATION_TIMEOUT' });
  h.answer(); await leader;
  assert.equal(h.calls.some(c => c.id === 2), false);
});

test('separate visual tabs and documents share one session prompt but get exact grants', async t => {
  const h = harness(); t.after(() => h.api.close());
  const tasks = [h.request(1), h.request(2, 'other'), h.request(3, 'tab', 'other')]
    .map(r => h.api.ensure(r, Date.now() + 2000));
  await tick(); assert.equal(h.messages.length, 1);
  h.answer(0); await Promise.all(tasks);
  assert.equal(h.calls.filter(c=>c.method.endsWith('accept')).length, 3);
  assert.equal(h.grants.size, 3);
  assert(h.calls.filter(c=>c.method.endsWith('accept')).slice(1).every(c=>c.args.source==='session_confirmation'));
});

test('revoked grant cannot be recreated silently from a prior answer', async t => {
  const h = harness(); t.after(() => h.api.close());
  const first = h.api.ensure(h.request(1), Date.now() + 1000);
  await tick(); h.answer(); await first;
  h.grants.clear();
  h.revoked.add('tab');
  await assert.rejects(h.api.ensure(h.request(2), Date.now() + 1000), {code:'VISUAL_ACCESS_REVOKED'});
  assert.equal(h.messages.length, 1);
  assert.equal(h.calls.filter(c=>c.method.endsWith('accept')).length, 1);
});

test('closing the client rejects leader and followers without accepting', async () => {
  const h = harness();
  const settled = Promise.allSettled([1, 2].map(id => h.api.ensure(h.request(id), Date.now() + 1000)));
  await tick(); h.api.close();
  for (const r of await settled) assert.equal(r.reason.code, 'VISUAL_AUTHORIZATION_CANCELLED');
  assert.equal(h.calls.length, 1);
  await assert.rejects(h.api.ensure(h.request(3), Date.now() + 1000), { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
});
