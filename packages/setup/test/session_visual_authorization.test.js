'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createMediaAuthorization } = require('../src/media_authorization');
const tick = () => new Promise(setImmediate);
const TTL = 10800000;

function session() {
  let clock = 1000000, sequence = 0;
  const messages = [], calls = [], challenges = new Map(), grants = new Map();
  const blocked = new Set(), cancelled = new Set();
  const api = createMediaAuthorization({ observation: 'visual', now: () => clock,
    isCancelled: id => cancelled.has(id), output: { write: line => messages.push(JSON.parse(line)) },
    invoke: async (method, args, timeout, id, deadline) => {
      calls.push({ method, args, id, deadline });
      if (blocked.has(args.tab_id)) throw Object.assign(new Error('Off'), { code: 'VISUAL_ACCESS_REVOKED' });
      const key = JSON.stringify([args.tab_id, args.document_id]);
      if (method.endsWith('.prepare')) {
        if (grants.get(key) > clock) return { granted: true, session_consent_version: 2 };
        const challenge = String(++sequence);
        challenges.set(challenge, { ...args, key });
        return { granted: false, challenge, session_consent_version: 2, grant_ttl_ms: args.grant_ttl_ms, grant_scope: args.grant_scope,
          grant_expires_at: args.grant_expires_at, consent_wait_version: 1, consent_expires_at: clock + 600000 };
      }
      const pending = challenges.get(args.challenge);
      challenges.delete(args.challenge);
      assert(pending);
      const expires_at = pending.grant_expires_at ?? clock + TTL;
      assert(expires_at > clock);
      grants.set(key, expires_at);
      return { granted: true, session_consent_version: 2, grant_scope: pending.grant_scope, expires_at };
    },
  });
  api.initialize({ elicitation: { form: {} } });
  return { api, messages, calls, grants, blocked, cancelled,
    now: () => clock, advance: ms => { clock += ms; },
    request: (id, tab = 'one', document = 'doc') => ({ id, params: { arguments: { tab_id: tab, document_id: document } } }),
    read(id, tab, doc) { return api.ensure(this.request(id, tab, doc), clock + 1000, { executionTimeoutMs: 1000 }); },
    answer(action = 'accept') {
      const prompt = messages.filter(m => m.method === 'elicitation/create').at(-1);
      api.receive({ id: prompt.id, result: { action, content: {} } });
    },
    prompts: () => messages.filter(m => m.method === 'elicitation/create'),
  };
}

test('one live-session approval covers new tabs, reload, reconnect and silent document-grant renewal', async t => {
  const h = session(); t.after(() => h.api.close());
  const first = h.read(1); await tick(); h.answer(); await first;
  h.advance(3600000);
  await h.read(2, 'two');
  await h.read(3, 'one', 'reloaded');
  h.grants.clear(); // Extension reconnect discards capture authority, not the live client's user decision.
  h.advance(3600000);
  await h.read(4, 'three');
  assert.equal(h.prompts().length, 1);
  assert(h.calls.filter(c => c.method.endsWith('.accept')).slice(1).every(c => c.args.source === 'session_confirmation'));
  assert(h.calls.filter(c => c.method.endsWith('.prepare')).every(c => c.args.grant_scope === 'session_observation'
    && c.args.grant_ttl_ms === TTL && c.args.grant_expires_at === undefined));
  h.advance(TTL + 1);
  await h.read(5, 'three');
  assert.equal(h.prompts().length, 1);
  assert.equal(h.grants.get(JSON.stringify(['three', 'doc'])), h.now() + TTL);
  h.advance(24 * 3600000);
  await h.read(6, 'three');
  assert.equal(h.prompts().length, 1);
  assert.equal(h.api.describe().consent_active, true);
  assert.equal(h.api.describe().consent_expires_at, undefined);
});

test('separate MCP sessions never share consent; closing a session discards its decision', async t => {
  const a = session(), b = session(); t.after(() => { a.api.close(); b.api.close(); });
  const first = a.read(1); await tick(); a.answer(); await first;
  const second = b.read(1); await tick();
  assert.equal(b.prompts().length, 1); b.answer('decline');
  await assert.rejects(second, { code: 'VISUAL_CLIENT_CONFIRMATION_DECLINED' });
  a.api.close();
  await assert.rejects(a.read(2, 'two'), { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
  assert.equal(a.calls.length, 3);
});

test('twenty simultaneous tabs get one confirmation', async t => {
  const h = session(); t.after(() => h.api.close());
  const tasks = Array.from({ length: 20 }, (_, id) => h.read(id, `tab-${id}`));
  await tick(); assert.equal(h.prompts().length, 1); h.answer(); await Promise.all(tasks);
  assert.equal(h.prompts().length, 1);
  assert.equal(h.grants.size, 20);
  assert.equal(new Set(h.grants.values()).size, 1);
});

test('manual Off rejects cached consent without another prompt or accept; other tabs still work', async t => {
  const h = session(); t.after(() => h.api.close());
  const first = h.read(1); await tick(); h.answer(); await first;
  h.grants.clear(); h.blocked.add('one');
  await assert.rejects(h.read(2, 'one', 'reload'), { code: 'VISUAL_ACCESS_REVOKED' });
  assert.equal(h.calls.filter(c => c.method.endsWith('.accept')).length, 1);
  await h.read(3, 'two');
  assert.equal(h.prompts().length, 1);
});

test('cancelled leader cannot authorize following tabs and late answers do nothing', async t => {
  const h = session(); t.after(() => h.api.close());
  const first = h.read(1); const rejected = assert.rejects(first, { code: 'VISUAL_AUTHORIZATION_CANCELLED' });
  await tick(); h.cancelled.add(1); h.api.cancel(1); await rejected;
  h.answer(); assert.equal(h.calls.length, 1);
  const next = h.read(2, 'two'); await tick(); assert.equal(h.prompts().length, 2);
  h.answer(); await next;
});

test('legacy page-only implementation cannot be mislabeled as session-wide consent', async () => {
  const messages = [];
  const api = createMediaAuthorization({ observation: 'visual', isCancelled: () => false,
    output: { write: line => messages.push(line) },
    invoke: async () => ({ granted: false, challenge: 'legacy', grant_ttl_ms: TTL }) });
  api.initialize({ elicitation: {} });
  await assert.rejects(api.ensure({ id: 1, params: { arguments: { tab_id: 'one', document_id: 'doc' } } }, Date.now() + 1000),
    { code: 'VISUAL_SESSION_CONSENT_UPGRADE_REQUIRED' });
  assert.equal(messages.length, 0); api.close();
});
