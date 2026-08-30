'use strict';

const { spawn } = require('node:child_process');
const path = require('node:path');
const { DEFAULT_PORT } = require('./broker');

function brokerUrl(environment = process.env) {
  const port = Number(environment.SACCADE_BROKER_PORT || DEFAULT_PORT);
  if (!Number.isSafeInteger(port) || port < 1024 || port > 65535) throw new Error('SACCADE_BROKER_PORT is invalid');
  return `http://127.0.0.1:${port}`;
}

async function request(route, {
  method = 'GET', body, headers: extraHeaders, timeoutMs = 10_000,
} = {}) {
  let response;
  try {
    response = await fetch(`${brokerUrl()}${route}`, {
      method,
      headers: { ...(body ? { 'content-type': 'application/json' } : {}), ...(extraHeaders || {}) },
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (cause) {
    const error = new Error(cause.name === 'TimeoutError' ? 'Broker request timed out' : 'Broker is unreachable');
    error.code = cause.name === 'TimeoutError' ? 'BROKER_TIMEOUT' : 'BROKER_UNREACHABLE';
    error.cause = cause;
    throw error;
  }
  const value = await response.json();
  if (!response.ok || value.ok === false) {
    const error = new Error(value.error?.message || `Broker returned HTTP ${response.status}`);
    error.code = value.error?.code || 'BROKER_ERROR';
    Object.assign(error, value.error || {});
    throw error;
  }
  return value;
}

async function healthy() {
  try { await request('/v1/health', { timeoutMs: 250 }); return true; }
  catch (_error) { return false; }
}

async function ensureBroker() {
  if (await healthy()) return;
  const bin = path.resolve(__dirname, '..', 'bin', 'saccade.js');
  const child = spawn(process.execPath, [bin, 'broker', '--child'], {
    detached: true,
    stdio: 'ignore',
    env: process.env,
  });
  child.unref();
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 50));
    if (await healthy()) return;
  }
  throw new Error('Node Broker did not become ready');
}

async function createSession(resumeToken) {
  await ensureBroker();
  return request('/v1/sessions', {
    method: 'POST',
    body: { ...(resumeToken ? { resume_token: resumeToken } : {}), upload_root: process.cwd() },
    timeoutMs: 2_000,
  });
}

function sessionId(session) {
  return typeof session === 'string' ? session : session.agent_session_id;
}

async function closeSession(session) {
  try {
    return await request(`/v1/sessions/${encodeURIComponent(sessionId(session))}`, {
      method: 'DELETE', headers: session.resume_token
        ? { 'x-saccade-session-token': session.resume_token } : undefined, timeoutMs: 1_000,
    });
  } catch (_error) { return null; }
}

async function resumeSession(session, { force = false } = {}) {
  if (!session || typeof session === 'string' || !session.resume_token) {
    throw Object.assign(new Error('Session has no in-memory resume proof'), { code: 'RESUME_UNAVAILABLE' });
  }
  await ensureBroker();
  const health = await request('/v1/health', { timeoutMs: 1_000 });
  if (!force && health.broker_epoch === session.broker_epoch) return { ...session, resumed: false };
  const resumed = await createSession(session.resume_token);
  if (resumed.agent_session_id !== session.agent_session_id) {
    throw Object.assign(new Error('Broker resumed a different Agent session'), { code: 'RESUME_IDENTITY_MISMATCH' });
  }
  Object.assign(session, resumed);
  return session;
}

async function rpc(session, method, params, timeoutMs, requestId) {
  const invoke = () => request('/v1/rpc', {
    method: 'POST',
    headers: typeof session === 'string' ? undefined : { 'x-saccade-session-token': session.resume_token },
    body: { agent_session_id: sessionId(session), method, params, timeout_ms: timeoutMs, request_id: requestId },
    timeoutMs: Math.min((timeoutMs || 10_000) + 250, 60_000),
  });
  try {
    return (await invoke()).result;
  } catch (error) {
    const rejectedBeforeDispatch = error.code === 'SESSION_OFFLINE';
    const transportFailure = ['BROKER_TIMEOUT', 'BROKER_UNREACHABLE'].includes(error.code);
    if (!rejectedBeforeDispatch && !transportFailure) throw error;
    let recovered = false;
    try {
      const before = typeof session === 'string' ? null : session.broker_epoch;
      const value = await resumeSession(session, { force: rejectedBeforeDispatch });
      recovered = value.resumed === true || value.broker_epoch !== before;
    } catch (_resumeError) {
      if (rejectedBeforeDispatch) throw error;
    }
    const idempotent = ['system.capabilities', 'tabs.list', 'truth.read'].includes(method);
    if (rejectedBeforeDispatch || idempotent) return (await invoke()).result;
    const unknown = new Error('Broker transport failed after a side-effecting request may have been dispatched');
    unknown.code = 'OUTCOME_UNKNOWN';
    unknown.stage = 'broker_transport';
    unknown.outcome = 'outcome_unknown';
    unknown.retry_safe = false;
    unknown.session_recovered = recovered;
    throw unknown;
  }
}

async function cancel(session, requestId) {
  return request('/v1/cancel', {
    method: 'POST',
    headers: typeof session === 'string' ? undefined : { 'x-saccade-session-token': session.resume_token },
    body: { agent_session_id: sessionId(session), request_id: requestId }, timeoutMs: 1_000,
  });
}

module.exports = {
  brokerUrl, cancel, closeSession, createSession, ensureBroker, healthy, request, resumeSession, rpc,
};
