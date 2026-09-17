'use strict';
// Real MCP stdio transport against an isolated, synthetic Broker. No browser,
// pixels, external network, model calls or production grants are involved.
const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const { spawn } = require('node:child_process');
const { once } = require('node:events');
const readline = require('node:readline');
const path = require('node:path');

async function client(t, { capabilities = { elicitation: { form: {} } }, protocol = '2025-11-25', consentExpiryMs = 600000, oldExtension = false, ownerScene = false } = {}) {
  const calls = [], messages = [], waiters = new Set();
  const server = http.createServer(async (req, res) => {
    let raw = '';
    for await (const chunk of req) raw += chunk;
    let value = { ok: true };
    if (req.url === '/v1/sessions' && req.method === 'POST') {
      value = { agent_session_id: 'test-session', resume_token: 'synthetic-proof', broker_epoch: 'test-epoch' };
    } else if (req.url === '/v1/rpc') {
      const call = JSON.parse(raw); calls.push(call);
      const scopedScene = ownerScene && call.method === 'visual.authorization.prepare'
        && call.params.scene_object_id === 'actor' && call.params.scene_generation === 'generation';
      const result = scopedScene ? { session_consent_version: 2, granted: true, access_kind: 'owner_scene_policy' }
        : call.method.endsWith('.prepare') ? { session_consent_version: 2, granted: false, challenge: 'synthetic-challenge', grant_ttl_ms: call.params.grant_ttl_ms, grant_scope: call.params.grant_scope, grant_expires_at: call.params.grant_expires_at, ...(oldExtension ? {} : { consent_wait_version: 1, consent_expires_at: Date.now() + consentExpiryMs }) }
        : call.method.endsWith('.accept') ? { session_consent_version: 2, granted: true, ...(calls.findLast(c=>c.method.endsWith('.prepare')).params.grant_scope ? { grant_scope: calls.findLast(c=>c.method.endsWith('.prepare')).params.grant_scope, expires_at: Date.now() + 10800000 } : {}) } : { synthetic_observation: true };
      value = { result };
    }
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify(value));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const child = spawn(process.execPath, [path.resolve(__dirname, '../bin/saccade.js'), 'mcp'], {
    env: { ...process.env, SACCADE_BROKER_PORT: String(server.address().port) },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  child.stderr.resume();
  const lines = readline.createInterface({ input: child.stdout });
  lines.on('line', line => {
    const message = JSON.parse(line); messages.push(message);
    for (const waiter of waiters) if (waiter.match(message)) waiter.finish(message);
  });
  const next = match => {
    const previous = messages.find(match);
    if (previous) return Promise.resolve(previous);
    return new Promise((resolve, reject) => {
      const waiter = { match, finish(value) { clearTimeout(timer); waiters.delete(waiter); resolve(value); } };
      const timer = setTimeout(() => { waiters.delete(waiter); reject(new Error('MCP test response timed out')); }, 3000);
      waiters.add(waiter);
    });
  };
  const send = value => child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', ...value })}\n`);
  t.after(async () => {
    child.stdin.end();
    const exited = once(child, 'exit');
    const timer = setTimeout(() => child.kill('SIGTERM'), 1500);
    await exited; clearTimeout(timer); lines.close();
    server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  });
  send({ id: 1, method: 'initialize', params: { protocolVersion: protocol, capabilities, clientInfo: { name: 'consent-test', version: '1' } } });
  const initialized = await next(m => m.id === 1);
  assert.equal(initialized.result.serverInfo.version, require('../package.json').version);
  send({ method: 'notifications/initialized' });
  return { send, next, calls, messages };
}

function observe(c, kind = 'visual', extra = {}) {
  c.send({ id: 2, method: 'tools/call', params: {
    name: `saccade.${kind}.read`, arguments: {
      tab_id: 'test-tab', document_id: 'test-document', timeout_ms: 1000,
      ...(kind === 'media' ? { mode: 'overview', object_id: 'video', media_id: 'generation' } : {}),
    }, ...extra,
  } });
}

test('real MCP stdio asks once across new tabs and refreshed documents in one session', async t => {
  const c = await client(t);
  observe(c);
  const prompt = await c.next(m => m.method === 'elicitation/create');
  assert.match(prompt.params.message, /authorized tabs for this session/);
  assert.match(prompt.params.message, /screenshots, video frames/);
  c.send({ id: prompt.id, result: { action: 'accept', content: {} } });
  const first = await c.next(m => m.id === 2);
  assert.notEqual(first.result.isError, true);
  for (const [id, tab_id, document_id] of [[10, 'new-tab', 'new-doc'], [11, 'test-tab', 'refreshed-doc'], [12, 'another-tab', 'another-doc']]) {
    c.send({ id, method: 'tools/call', params: { name: 'saccade.visual.read', arguments: { tab_id, document_id } } });
    const response = await c.next(m => m.id === id);
    assert.notEqual(response.result?.isError, true);
    assert.equal(response.error, undefined);
  }
  assert.equal(c.messages.filter(m => m.method === 'elicitation/create').length, 1);
  const derived = c.calls.filter(call => call.method === 'visual.authorization.prepare').slice(1);
  assert.equal(derived.length, 4); // Post-answer revalidation plus three later documents.
  assert(derived.every(c => c.params.grant_scope === 'session_observation' && c.params.grant_ttl_ms === 10800000));
  assert(derived.every(c => c.params.grant_expires_at === undefined));
  assert.equal(c.calls.filter(call => call.method === 'visual.read').length, 4);
  assert(c.calls.filter(call => call.method.endsWith('.accept')).slice(1).every(c => c.params.source === 'session_confirmation'));
  c.send({ id: 13, method: 'tools/call', params: { name: 'saccade.media.read', arguments: {
    tab_id: 'new-tab', document_id: 'new-doc', mode: 'overview', object_id: 'video', media_id: 'generation' } } });
  const media = await c.next(m => m.id === 13);
  assert.equal(media.error, undefined); assert.notEqual(media.result?.isError, true);
  assert.equal(c.messages.filter(m => m.method === 'elicitation/create').length, 1);
  assert.equal(c.calls.findLast(c => c.method === 'media.authorization.accept').params.source, 'session_confirmation');
});

test('owner-scoped scene sequence skips elicitation even without host form support; snapshots do not', async t => {
  const c = await client(t, { ownerScene: true, capabilities: {} });
  observe(c, 'visual', { arguments: { tab_id: 'test-tab', document_id: 'test-document',
    mode: 'sequence', object_id: 'actor', scene_generation: 'generation', duration_ms: 100 } });
  const response = await c.next(m => m.id === 2);
  assert.notEqual(response.result.isError, true);
  assert.deepEqual(c.calls.map(call => call.method), ['visual.authorization.prepare', 'visual.read']);
  assert.equal(c.calls[0].params.scene_object_id, 'actor');
  assert.equal(c.calls[0].params.scene_generation, 'generation');
  assert.equal(c.messages.some(m => m.method === 'elicitation/create'), false);
  c.send({ id: 3, method: 'tools/call', params: { name: 'saccade.visual.read',
    arguments: { tab_id: 'test-tab', document_id: 'test-document' } } });
  const snapshot = await c.next(m => m.id === 3);
  assert.equal(snapshot.result.structuredContent.error.code, 'VISUAL_CLIENT_CONFIRMATION_UNSUPPORTED');
  assert.equal(c.calls.length, 3);
});

for (const mode of ['snapshot', undefined]) {
  test(`owner scene still ${mode || 'default'} bypasses prompts but keeps exact scene scope`, async t => {
    const c = await client(t, { ownerScene: true, capabilities: {} });
    observe(c, 'visual', { arguments: {tab_id:'test-tab',document_id:'test-document',object_id:'actor',scene_generation:'generation',...(mode ? {mode} : {})} });
    const response = await c.next(m => m.id === 2);
    assert.notEqual(response.result.isError, true);
    assert.equal(c.calls[0].params.scene_mode, 'snapshot');
    assert.deepEqual(c.calls.map(x=>x.method), ['visual.authorization.prepare','visual.read']);
    assert.equal(c.messages.some(m=>m.method==='elicitation/create'), false);
  });
}

for (const kind of ['visual', 'media']) {
  test(`${kind}: one explicit client acceptance starts one acquisition deadline without a second checkbox`, async t => {
    const c = await client(t); observe(c, kind);
    const prompt = await c.next(m => m.method === 'elicitation/create');
    assert.deepEqual(prompt.params.requestedSchema, { type: 'object', properties: {} });
    c.send({ id: prompt.id, result: { action: 'accept', content: {} } });
    const response = await c.next(m => m.id === 2);
    assert.equal(response.error, undefined); assert.notEqual(response.result.isError, true);
    assert.deepEqual(c.calls.map(c => c.method), [`${kind}.authorization.prepare`, `${kind}.authorization.prepare`, `${kind}.authorization.accept`, `${kind}.read`]);
    assert.equal(c.calls[0].params.wait_for_consent, true);
    assert.equal(c.calls[1].deadline_at, c.calls[2].deadline_at);
    assert.equal(c.calls[2].deadline_at, c.calls[3].deadline_at);
    assert(c.calls[1].deadline_at >= c.calls[0].deadline_at);
  });
  for (const [answer, suffix] of [
    [{ action: 'decline' }, 'CLIENT_CONFIRMATION_DECLINED'],
    [{ action: 'cancel' }, 'AUTHORIZATION_CANCELLED'],
    [{ action: 'accept', content: { allow: false } }, 'AUTHORIZATION_NOT_GRANTED'],
    [{ action: 'accept', content: { allow: 'true' } }, 'AUTHORIZATION_INVALID_RESPONSE'],
  ]) {
    test(`${kind}: stdio ${suffix} is an actionable tool error with no capture`, async t => {
      const c = await client(t); observe(c, kind);
      const prompt = await c.next(m => m.method === 'elicitation/create');
      c.send({ id: prompt.id, result: answer });
      const response = await c.next(m => m.id === 2);
      assert.equal(response.error, undefined); assert.equal(response.result.isError, true);
      assert.equal(response.result.structuredContent.error.code, `${kind.toUpperCase()}_${suffix}`);
      assert.equal(response.result.structuredContent.error.retry_safe, false);
      assert.equal(c.calls.length, 1);
      assert.equal(c.messages.filter(m => m.method === 'elicitation/create').length, 1);
    });
  }
}

test('legacy protocol cannot request consent even if form capability is advertised', async t => {
  const c = await client(t, { protocol: '2025-03-26' }); observe(c);
  const response = await c.next(m => m.id === 2);
  assert.equal(response.result.structuredContent.error.code, 'VISUAL_CLIENT_CONFIRMATION_UNSUPPORTED');
  assert.equal(c.messages.some(m => m.method === 'elicitation/create'), false);
  assert.equal(c.calls.length, 1);
});

test('unnegotiated host metadata still requires a client answer', async t => {
  const c = await client(t);
  observe(c, 'visual', { _meta: { 'io.saccade/visual-intent': { kind: 'explicit_request', tab_id: 'test-tab', document_id: 'test-document' } } });
  const prompt = await c.next(m => m.method === 'elicitation/create');
  c.send({ id: prompt.id, result: { action: 'decline' } });
  await c.next(m => m.id === 2);
  assert.equal(c.calls.length, 1);
});

for (const [code, suffix] of [[-32601, 'UNSUPPORTED'], [-32602, 'INVALID_REQUEST'], [-32603, 'FAILED']]) {
  test(`elicitation RPC error ${code} is not attributed to user decline`, async t => {
    const c = await client(t); observe(c);
    const prompt = await c.next(m => m.method === 'elicitation/create');
    c.send({ id: prompt.id, error: { code, message: 'sensitive-client-detail' } });
    const response = await c.next(m => m.id === 2);
    assert.equal(response.result.structuredContent.error.code, `VISUAL_CLIENT_CONFIRMATION_${suffix}`);
    assert.doesNotMatch(JSON.stringify(response), /sensitive-client-detail/);
    assert.equal(c.calls.length, 1);
  });
}

test('negotiated and exact host intent needs no redundant prompt', async t => {
  const c = await client(t, { capabilities: { experimental: { 'io.saccade/visual-intent': 1 } } });
  observe(c, 'visual', { _meta: { 'io.saccade/visual-intent': { kind: 'explicit_request', tab_id: 'test-tab', document_id: 'test-document' } } });
  const response = await c.next(m => m.id === 2);
  assert.notEqual(response.result.isError, true);
  assert.equal(c.messages.some(m => m.method === 'elicitation/create'), false);
  assert.equal(c.calls[1].params.source, 'explicit_request');
  assert.equal(c.calls.length, 3);
});

test('default snapshot survives an eleven-second human wait then starts bounded acquisition', async t => {
  const c = await client(t);
  const start = Date.now();
  observe(c, 'visual', { arguments: { tab_id: 'test-tab', document_id: 'test-document' } });
  const prompt = await c.next(m => m.method === 'elicitation/create');
  const originalDeadline = c.calls[0].deadline_at;
  assert(originalDeadline >= start + 29000);
  assert(originalDeadline <= Date.now() + 30000);
  // Real subprocess timing catches the old adapter's ten-second prompt expiry.
  await new Promise(resolve => setTimeout(resolve, 11000));
  assert.equal(c.messages.some(m => m.id === 2), false);
  const acceptedAt = Date.now();
  c.send({ id: prompt.id, result: { action: 'accept', content: { allow: true } } });
  const response = await c.next(m => m.id === 2);
  assert.equal(response.error, undefined);
  assert.notEqual(response.result.isError, true);
  assert.deepEqual(c.calls.map(call => call.method), ['visual.authorization.prepare', 'visual.authorization.prepare', 'visual.authorization.accept', 'visual.read']);
  assert(c.calls[1].deadline_at >= acceptedAt + 29000);
  assert.equal(c.calls[2].deadline_at, c.calls[1].deadline_at);
  assert(c.calls[3].deadline_at <= c.calls[2].deadline_at);
  assert(c.calls[3].deadline_at >= acceptedAt);
  assert(c.calls[3].deadline_at <= Date.now() + 10000);
  assert.equal(c.messages.filter(m => m.method === 'elicitation/create').length, 1);
});

test('expired document challenge does not expire the prompt or require a second answer', async t => {
  const c = await client(t, { consentExpiryMs: 40 }); observe(c);
  const prompt = await c.next(m => m.method === 'elicitation/create');
  await new Promise(resolve => setTimeout(resolve, 80));
  assert.equal(c.messages.some(m => m.method === 'notifications/cancelled'), false);
  assert.equal(c.calls.length, 1);
  c.send({ id: prompt.id, result: { action: 'accept', content: { allow: true } } });
  const response = await c.next(m => m.id === 2);
  assert.notEqual(response.result.isError, true);
  c.send({ id: 3, method: 'ping' }); await c.next(m => m.id === 3);
  assert.equal(c.messages.filter(m => m.method === 'elicitation/create').length, 1);
  assert.deepEqual(c.calls.map(c => c.method), ['visual.authorization.prepare', 'visual.authorization.prepare', 'visual.authorization.accept', 'visual.read']);
  assert.equal(c.calls[1].deadline_at, c.calls[2].deadline_at);
});

test('parent cancellation withdraws the consent prompt and a late answer cannot capture', async t => {
  const c = await client(t); observe(c);
  const prompt = await c.next(m => m.method === 'elicitation/create');
  c.send({ method: 'notifications/cancelled', params: { requestId: 2 } });
  const cancelled = await c.next(m => m.method === 'notifications/cancelled');
  assert.equal(cancelled.params.requestId, prompt.id);
  c.send({ id: prompt.id, result: { action: 'accept', content: { allow: true } } });
  c.send({ id: 3, method: 'ping' }); await c.next(m => m.id === 3);
  assert.equal(c.messages.filter(m => m.id === prompt.id).length, 1);
  assert.equal(c.calls.length, 1);
});

test('an old confirmation protocol fails before presenting a doomed prompt', async t => {
  const c = await client(t, { oldExtension: true }); observe(c);
  const response = await c.next(m => m.id === 2);
  assert.equal(response.result.structuredContent.error.code, 'VISUAL_CONSENT_WAIT_UPGRADE_REQUIRED');
  assert.equal(c.messages.some(m => m.method === 'elicitation/create'), false);
  assert.equal(c.calls.length, 1);
});
