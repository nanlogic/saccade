'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createMediaAuthorization, INTENT_KEY } = require('../src/media_authorization');
function harness(caps = {}, meta, observation = 'media', options = {}) {
  const calls = [], messages = [];
  const api = createMediaAuthorization({ output: { write: (line) => messages.push(JSON.parse(line)) },
    invoke: async (method, args, timeout, id, deadline) => {
      calls.push({ method, args, timeout, id, deadline });
      if (method.endsWith('prepare')) return { session_consent_version: 2, granted: false, challenge: 'private-challenge', grant_scope: args.grant_scope, grant_expires_at: args.grant_expires_at, ...(args.grant_ttl_ms ? { grant_ttl_ms: args.grant_ttl_ms } : {}), ...(args.wait_for_consent ? { consent_wait_version: 1, consent_expires_at: (options.now || Date.now)() + 600000 } : {}) };
      const prepared = calls.findLast(call => call.id === id && call.method.endsWith('prepare')).args;
      return { session_consent_version: 2, granted: true, ...(prepared.grant_scope ? { grant_scope: prepared.grant_scope, expires_at: prepared.grant_expires_at ?? (options.now || Date.now)() + 10800000 } : {}) };
    }, isCancelled: () => false, observation, ...options });
  api.initialize(caps);
  return { api, calls, messages, request: { id: 3, params: { arguments: { tab_id: '7', document_id: 'doc' }, _meta: meta } } };
}
test('host-attested explicit request and previous confirmation need no second prompt', async () => {
  for (const kind of ['explicit_request','confirmation']) {
    const h = harness({ experimental: { [INTENT_KEY]: 1 } }, { [INTENT_KEY]: { kind, tab_id: '7', document_id: 'doc' } });
    const deadline = Date.now() + 30000;
    await h.api.ensure(h.request, deadline);
    assert.equal(h.messages.length, 0); assert.equal(h.calls.length, 2);
    assert.equal(h.calls[1].args.source, kind);
    assert(h.calls.every((call) => call.deadline === deadline));
  }
});

test('visual consent uses its own host attestation and cannot borrow video intent', async () => {
  const key = 'io.saccade/visual-intent';
  const intent = { kind: 'explicit_request', tab_id: '7', document_id: 'doc' };
  const h = harness({ experimental: { [key]: 1 } }, { [key]: intent }, 'visual');
  const deadline = Date.now() + 10000;
  await h.api.ensure(h.request, deadline);
  assert.deepEqual(h.calls.map(call => call.method), ['visual.authorization.prepare', 'visual.authorization.accept']);
  assert(h.calls.every(call => call.deadline === deadline));
  assert.equal(h.messages.length, 0);
  const wrong = harness({ experimental: { [INTENT_KEY]: 1 } }, { [INTENT_KEY]: intent }, 'visual');
  await assert.rejects(wrong.api.ensure(wrong.request, deadline), { code: 'VISUAL_CLIENT_CONFIRMATION_UNSUPPORTED' });
});

test('visual confirmation describes screenshot privacy without requesting popup changes', async () => {
  const h = harness({ elicitation: {} }, undefined, 'visual');
  const task = h.api.ensure(h.request, Date.now() + 1000);
  await new Promise(setImmediate);
  assert.match(h.messages[0].params.message, /personal information/);
  assert.match(h.messages[0].params.message, /until this session ends/);
  assert.equal(h.calls[0].args.grant_ttl_ms, 10800000);
  assert.equal(h.calls[0].args.grant_scope, 'session_observation');
  assert.match(h.messages[0].params.message, /One approval covers/);
  assert.match(h.messages[0].params.message, /new pages, refreshes/);
  assert.doesNotMatch(h.messages[0].params.message, /see tab 7|Navigation, reconnect/);
  assert.doesNotMatch(h.messages[0].params.message, /popup/);
  h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: { allow: true } } });
  await task;
  assert.equal(h.calls[1].args.source, 'client_confirmation');
});
test('legacy Extension duration cannot produce a misleading session prompt', async () => {
  for (const grant_ttl_ms of [undefined, 900000, 10800001]) {
    const h = harness({ elicitation: {} }, undefined, 'visual', {
      invoke: async () => ({ session_consent_version: 2, granted: false, challenge: 'old', grant_ttl_ms }),
    });
    await assert.rejects(h.api.ensure(h.request, Date.now() + 1000), { code: 'VISUAL_CONSENT_DURATION_UPGRADE_REQUIRED' });
    assert.equal(h.messages.length, 0);
  }
});
test('video consent discloses unified live-session scope', async () => {
  const h = harness({ elicitation: {} });
  const pending = h.api.ensure(h.request, Date.now() + 1000);
  await new Promise(setImmediate);
  assert.match(h.messages[0].params.message, /until this session ends/);
  assert.match(h.messages[0].params.message, /screenshots, video frames/);
  assert.equal(h.calls[0].args.grant_ttl_ms, 10800000);
  h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: {} } });
  await pending;
});
test('chat confirmation accepts only matched client response with explicit true', async () => {
  const h = harness({ elicitation: {} }); const task = h.api.ensure(h.request, Date.now() + 1000);
  await new Promise(setImmediate);
  assert.equal(h.messages[0].method, 'elicitation/create');
  assert.equal(h.api.receive({ id: 'wrong', result: { action: 'accept', content: { allow: true } } }), false);
  h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: { allow: true } } });
  await task; assert.equal(h.calls[1].args.source, 'client_confirmation');
});

test('URL-only elicitation does not advertise form consent', async () => {
  const h = harness({ elicitation: { url: {} } });
  await assert.rejects(h.api.ensure(h.request, Date.now() + 1000), { code: 'MEDIA_CLIENT_CONFIRMATION_UNSUPPORTED' });
  assert.equal(h.messages.length, 0);
  const form = harness({ elicitation: { form: {} } });
  const task = form.api.ensure(form.request, Date.now() + 1000);
  await new Promise(setImmediate);
  form.api.receive({ id: form.messages[0].id, result: { action: 'accept', content: { allow: true } } });
  await task;
});
test('decline, missing response data, cancellation and late acceptance never issue authorization', async () => {
  for (const answer of [{ action: 'decline' }, { action: 'accept' }, null]) {
    const h = harness({ elicitation: {} });
    const task = h.api.ensure(h.request, Date.now() + 1000); const rejected = assert.rejects(task);
    await new Promise(setImmediate);
    if (answer) h.api.receive({ id: h.messages[0].id, result: answer }); else h.api.cancel(3);
    await rejected; assert.equal(h.calls.length, 1);
    assert.equal(h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: { allow: true } } }), false);
  }
});
test('Agent arguments, unadvertised metadata and mismatched scope cannot attest user intent', async () => {
  const intent = { kind: 'explicit_request', tab_id: '7', document_id: 'doc' };
  const h = harness(); h.request.params.arguments.consent = true;
  h.request.params.arguments._meta = { [INTENT_KEY]: intent };
  await assert.rejects(h.api.ensure(h.request, Date.now() + 1000), { code: 'MEDIA_CLIENT_CONFIRMATION_UNSUPPORTED' });
  for (const [caps, meta] of [[{}, intent], [{ experimental: { [INTENT_KEY]: 1 } }, { ...intent, tab_id: '8' }]]) {
    const other = harness(caps, { [INTENT_KEY]: meta });
    await assert.rejects(other.api.ensure(other.request, Date.now() + 1000)); assert.equal(other.calls.length, 1);
  }
});
test('client disconnect or prompt timeout releases pending request without granting', async () => {
  const h = harness({ elicitation: {} });
  const task = h.api.ensure(h.request, Date.now() + 1000); const rejected = assert.rejects(task);
  await new Promise(setImmediate); h.api.close(); await rejected; assert.equal(h.calls.length, 1);
  const timeout = harness({ elicitation: {} });
  await assert.rejects(timeout.api.ensure(timeout.request, Date.now() + 15), { code: 'MEDIA_AUTHORIZATION_TIMEOUT' });
  assert.equal(timeout.calls.length, 1);
});

test('visual confirmation has a bounded wait and ignores late acceptance', async () => {
  const h = harness({ elicitation: { form: {} } }, undefined, 'visual', { confirmationTimeoutMs: 20 });
  await assert.rejects(h.api.ensure(h.request, Date.now() + 30000), error => {
    assert.equal(error.code, 'VISUAL_AUTHORIZATION_TIMEOUT');
    assert.match(error.message, /late answer cannot grant access/);
    return true;
  });
  assert.equal(h.calls.length, 1);
  assert.equal(h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: { allow: true } } }), false);
});

test('legacy callers can explicitly cap confirmation within their original deadline', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const h = harness({ elicitation: { form: {} } }, undefined, 'visual', { confirmationTimeoutMs: 20000 });
  let settled = false;
  const task = h.api.ensure(h.request, Date.now() + 30000);
  task.then(() => { settled = true; }, () => { settled = true; });
  const rejected = assert.rejects(task, { code: 'VISUAL_AUTHORIZATION_TIMEOUT' });
  await new Promise(setImmediate);
  t.mock.timers.tick(19999);
  await new Promise(setImmediate);
  assert.equal(settled, false);
  t.mock.timers.tick(1);
  await rejected;
  assert.equal(h.calls.length, 1);
  assert.equal(h.api.receive({ id: h.messages[0].id, result: { action: 'accept', content: { allow: true } } }), false);
});

test('an explicit short request deadline still bounds visual confirmation', async () => {
  const h = harness({ elicitation: { form: {} } }, undefined, 'visual');
  const deadline = Date.now() + 20;
  await assert.rejects(h.api.ensure(h.request, deadline), { code: 'VISUAL_AUTHORIZATION_TIMEOUT' });
  assert.equal(h.calls.length, 1);
  assert.equal(h.calls[0].deadline, deadline);
});

test('existing visual grant uses the fast path without a confirmation timer', async () => {
  const h = harness({ elicitation: { form: {} } }, undefined, 'visual', {
    confirmationTimeoutMs: 1, invoke: async () => ({ session_consent_version: 2, granted: true }),
  });
  await h.api.ensure(h.request, Date.now() + 30000);
  assert.equal(h.messages.length, 0);
});

test('confirmation outcomes distinguish client decline from cancel and invalid data',async()=>{
 for(const [answer,code] of [[{action:'decline'},'VISUAL_CLIENT_CONFIRMATION_DECLINED'],[{action:'cancel'},'VISUAL_AUTHORIZATION_CANCELLED'],[{action:'accept',content:{unexpected:true}},'VISUAL_AUTHORIZATION_INVALID_RESPONSE'],[{action:'accept',content:{allow:false}},'VISUAL_AUTHORIZATION_NOT_GRANTED']]) {
  const h=harness({elicitation:{}},undefined,'visual');const task=h.api.ensure(h.request,Date.now()+1000);const rejected=assert.rejects(task,{code});await new Promise(setImmediate);
  h.api.receive({id:h.messages[0].id,result:answer});await rejected;assert.equal(h.calls.length,1);
 }
});

test('client decline explains the prompt-policy boundary without granting or retrying', async () => {
  for (const observation of ['media', 'visual']) {
    const h = harness({ elicitation: { form: {} } }, undefined, observation);
    const task = h.api.ensure(h.request, Date.now() + 1000);
    const rejected = assert.rejects(task, error => {
      assert.equal(error.code, `${observation.toUpperCase()}_CLIENT_CONFIRMATION_DECLINED`);
      assert.equal(error.retry_safe, false);
      assert.match(error.message, /does not establish whether a person saw/);
      assert.match(error.message, /approval_policy\.granular\.mcp_elicitations/);
      assert.match(error.message, /Do not enable Full Access, retry automatically/);
      return true;
    });
    await new Promise(setImmediate);
    h.api.receive({ id: h.messages[0].id, result: { action: 'decline' } });
    await rejected;
    assert.equal(h.calls.length, 1);
    assert.equal(h.messages.length, 1);
  }
});
