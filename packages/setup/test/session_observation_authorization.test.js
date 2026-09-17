'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createMediaAuthorization, createSessionObservationConsent } = require('../src/media_authorization');
const tick = () => new Promise(setImmediate);
const TTL = 10800000;

function session() {
  let clock = 1000000, sequence = 0;
  const state = createSessionObservationConsent();
  const messages = [], calls = [], grants = new Map(), challenges = new Map(), blocked = new Set(), cancelled = new Set();
  const apis = {};
  for (const observation of ['media', 'visual']) {
    apis[observation] = createMediaAuthorization({ observation, sessionConsent: state, now: () => clock,
      isCancelled: id => cancelled.has(id), output: { write: line => messages.push(JSON.parse(line)) },
      invoke: async (method, args) => {
        calls.push({ method, args });
        if (blocked.has(`${observation}:${args.tab_id}`)) throw Object.assign(new Error('Off'), { code: `${observation.toUpperCase()}_ACCESS_REVOKED` });
        const key = JSON.stringify([observation, args.tab_id, args.document_id]);
        if (method.endsWith('.prepare')) {
          if (grants.get(key) > clock) return { granted: true, session_consent_version: 2 };
          const challenge = String(++sequence);
          challenges.set(challenge, { ...args, key, expires: clock + 600000 });
          return { granted: false, challenge, session_consent_version: 2,
            grant_scope: args.grant_scope, grant_ttl_ms: args.grant_ttl_ms,
            consent_wait_version: 1, consent_expires_at: clock + 600000 };
        }
        const pending = challenges.get(args.challenge);
        challenges.delete(args.challenge);
        assert(pending);
        assert(pending.expires > clock, 'expired challenge must never be accepted');
        const expires_at = clock + TTL;
        grants.set(key, expires_at);
        return { granted: true, session_consent_version: 2, grant_scope: pending.grant_scope, expires_at };
      },
    });
    apis[observation].initialize({ elicitation: { form: {} } });
  }
  return { apis, state, calls, grants, blocked, cancelled,
    invalidateChallenges() { challenges.clear(); },
    prompts: () => messages.filter(m => m.method === 'elicitation/create'),
    advance(ms) { clock += ms; },
    read(kind, id, tab = 'one', document = 'doc', meta) {
      return apis[kind].ensure({ id, params: { arguments: { tab_id: tab, document_id: document }, ...(meta ? { _meta: meta } : {}) } },
        clock + 1000, { executionTimeoutMs: 1000 });
    },
    answer(action = 'accept', prompt = this.prompts().at(-1)) {
      const response = { id: prompt.id, result: { action, content: {} } };
      return apis.media.receive(response) || apis.visual.receive(response);
    },
    close() { apis.media.close(); apis.visual.close(); },
  };
}

for (const kind of ['media', 'visual']) {
  test(`${kind} revalidates the exact document after the human wait without another prompt`, async t => {
    const h = session(); t.after(() => h.close());
    const first = h.read(kind, 1);
    await tick();
    // Activation/reconnect can invalidate a challenge before its timer expires.
    h.invalidateChallenges();
    h.answer();
    await first;
    assert.equal(h.prompts().length, 1);
    assert.deepEqual(h.calls.map(call => call.method), [
      `${kind}.authorization.prepare`, `${kind}.authorization.prepare`, `${kind}.authorization.accept`,
    ]);
    assert(h.calls.every(call => call.args.tab_id === 'one' && call.args.document_id === 'doc'));
    assert.equal(h.calls.at(-1).args.source, 'session_confirmation');
    assert.equal(h.grants.size, 1);
  });

  test(`${kind} Off during the human wait denies fresh authorization without losing the decision`, async t => {
    const h = session(); t.after(() => h.close());
    const first = h.read(kind, 1);
    const rejected = assert.rejects(first, { code: `${kind.toUpperCase()}_ACCESS_REVOKED` });
    await tick(); h.blocked.add(`${kind}:one`); h.answer(); await rejected;
    assert.equal(h.calls.filter(call => call.method.endsWith('.accept')).length, 0);
    assert.equal(h.grants.size, 0);
    assert(h.state.decision);
    await h.read(kind, 2, 'two');
    assert.equal(h.prompts().length, 1);
  });
}

for (const [firstKind, nextKind] of [['media', 'visual'], ['visual', 'media']]) {
  test(`${firstKind} approval covers ${nextKind}, refreshed pages and renewals beyond 24 hours`, async t => {
    const h = session(); t.after(() => h.close());
    const first = h.read(firstKind, 1); await tick(); h.answer(); await first;
    await h.read(nextKind, 2, 'two');
    h.grants.clear(); // A verified Extension reconnect keeps only the live MCP decision.
    h.advance(TTL + 1);
    await h.read(nextKind, 3, 'two', 'reload');
    h.advance(24 * 3600000);
    await h.read(firstKind, 4, 'three');
    assert.equal(h.prompts().length, 1);
    assert.match(h.prompts()[0].params.message, /screenshots, video frames and captions/);
    assert.doesNotMatch(h.prompts()[0].params.message, /3 hours|15 minutes|10 minutes|Confirm within/);
    assert(h.calls.filter(c => c.method.endsWith('.accept')).slice(1).every(c => c.args.source === 'session_confirmation'));
    for (const api of Object.values(h.apis)) {
      assert.equal(api.describe().consent_active, true);
      assert.equal(api.describe().consent_lifetime, 'live_session');
      assert.equal(api.describe().consent_expires_at, undefined);
    }
  });
}

test('concurrent cross-kind followers share one prompt even after a day and derive fresh document grants', async t => {
  const h = session(); t.after(() => h.close());
  const tasks = [h.read('visual', 1, 'one'), h.read('media', 2, 'two'), h.read('visual', 3, 'three')];
  await tick(); assert.equal(h.prompts().length, 1); h.advance(86400000); h.answer(); await Promise.all(tasks);
  assert.equal(h.prompts().length, 1);
  assert.equal(h.grants.size, 3);
  assert.deepEqual(h.calls.filter(c => c.method.endsWith('.accept')).map(c => c.args.source),
    ['session_confirmation', 'session_confirmation', 'session_confirmation']);
});

test('cancelled cross-kind follower does not cancel the approved leader', async t => {
  const h = session(); t.after(() => h.close());
  const first = h.read('visual', 1);
  const second = h.read('media', 2, 'two');
  const rejected = assert.rejects(second, { code: 'MEDIA_AUTHORIZATION_CANCELLED' });
  await tick(); h.cancelled.add(2); h.apis.media.cancel(2); await rejected;
  h.answer(); await first;
  assert.equal(h.grants.size, 1);
  await h.read('media', 3, 'three');
  assert.equal(h.prompts().length, 1);
});

test('leader cancellation leaves no shared decision and a late response cannot grant one', async t => {
  const h = session(); t.after(() => h.close());
  const first = h.read('visual', 1), second = h.read('media', 2, 'two');
  const rejected = [assert.rejects(first, { code: 'VISUAL_AUTHORIZATION_CANCELLED' }),
    assert.rejects(second, /AUTHORIZATION_CANCELLED/)];
  await tick(); h.cancelled.add(1); h.apis.visual.cancel(1); await Promise.all(rejected);
  assert.equal(h.answer(), false);
  assert.equal(h.state.decision, undefined);
  assert.equal(h.grants.size, 0);
  const next = h.read('media', 3); await tick(); assert.equal(h.prompts().length, 2);
  h.answer(); await next;
});

test('closing a pending session rejects both kinds and late answers stay inert', async () => {
  const h = session();
  const first = h.read('media', 1), second = h.read('visual', 2, 'two');
  const rejected = [assert.rejects(first, /AUTHORIZATION_CANCELLED/), assert.rejects(second, /AUTHORIZATION_CANCELLED/)];
  await tick(); h.close(); await Promise.all(rejected);
  assert.equal(h.answer(), false);
  assert.equal(h.state.decision, undefined);
  assert.equal(h.grants.size, 0);
  await assert.rejects(h.read('visual', 3), { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
});

test('independent sessions require independent approval and close discards the shared decision', async t => {
  const a = session(), b = session(); t.after(() => { a.close(); b.close(); });
  const first = a.read('media', 1); await tick(); a.answer(); await first;
  const second = b.read('visual', 1); await tick(); assert.equal(b.prompts().length, 1); b.answer('decline');
  await assert.rejects(second, { code: 'VISUAL_CLIENT_CONFIRMATION_DECLINED' });
  a.close();
  assert.equal(a.apis.media.describe().consent_active, false);
  assert.equal(a.apis.visual.describe().consent_active, false);
});

for (const kind of ['media', 'visual']) {
  test(`${kind} Off rejects renewal without a new prompt; the other kind remains available`, async t => {
    const h = session(); t.after(() => h.close());
    const first = h.read(kind, 1); await tick(); h.answer(); await first;
    h.grants.clear(); h.blocked.add(`${kind}:one`);
    await assert.rejects(h.read(kind, 2), { code: `${kind.toUpperCase()}_ACCESS_REVOKED` });
    await h.read(kind === 'media' ? 'visual' : 'media', 3);
    assert.equal(h.prompts().length, 1);
    assert.equal(h.calls.filter(c => c.method.endsWith('.accept')).length, 2);
  });

  test(`${kind} v1 runtime fails closed even when it reports an existing grant`, async () => {
    for (const granted of [false, true]) {
      const messages = [];
      const api = createMediaAuthorization({ observation: kind, isCancelled: () => false,
        output: { write: line => messages.push(line) }, invoke: async () => ({ granted,
          session_consent_version: 1, grant_scope: 'session', grant_ttl_ms: TTL, challenge: 'legacy' }) });
      api.initialize({ elicitation: {} });
      await assert.rejects(api.ensure({ id: 1, params: { arguments: { tab_id: 'one', document_id: 'doc' } } }, Date.now() + 1000),
        { code: `${kind.toUpperCase()}_SESSION_CONSENT_UPGRADE_REQUIRED` });
      assert.equal(messages.length, 0); api.close();
    }
  });
}

test('host-attested page intent cannot establish the shared session decision', async t => {
  const h = session(); t.after(() => h.close());
  h.apis.media.initialize({ elicitation: {}, experimental: { 'io.saccade/media-intent': 1 } });
  await h.read('media', 1, 'one', 'doc', { 'io.saccade/media-intent': {
    kind: 'explicit_request', tab_id: 'one', document_id: 'doc' } });
  assert.equal(h.state.decision, undefined);
  assert.equal(h.calls[0].args.grant_scope, undefined);
  assert.equal(h.calls[1].args.source, 'explicit_request');
  const next = h.read('visual', 2, 'two'); await tick(); assert.equal(h.prompts().length, 1);
  h.answer(); await next;
});
