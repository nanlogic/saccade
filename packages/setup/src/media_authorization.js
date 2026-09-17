'use strict';
const { randomUUID } = require('node:crypto');
const INTENT_KEY = 'io.saccade/media-intent';
const VISUAL_GRANT_MS = 3 * 60 * 60 * 1000;
// One per serveMcp instance, never global, serialized or resumed by a new client.
function createSessionObservationConsent() {
  return { decision: undefined, closed: false, authorizing: new Map(), followers: new Set() };
}
const fail = (code) => Object.assign(new Error(code), { code, stage: 'media_authorization', retry_safe: false });

// Host responses and host-added request metadata never come from tool arguments.
// This is a client attestation, not a claim that MCP transports the chat transcript.
function createMediaAuthorization({ output, invoke, isCancelled, now = Date.now, observation = 'media',
  sessionConsent = createSessionObservationConsent(),
  confirmationTimeoutMs = Infinity }) {
  if (!['media', 'visual'].includes(observation)) throw new Error('Invalid observation authorization kind');
  const intentKey = observation === 'media' ? INTENT_KEY : 'io.saccade/visual-intent';
  const failure = (code) => {
    const error = fail(code);
    if (observation === 'visual') { error.code = code.replace(/^MEDIA_/, 'VISUAL_'); error.message = error.code; error.stage = 'visual_authorization'; }
    if (code === 'MEDIA_CLIENT_CONFIRMATION_DECLINED') {
      error.message = `${error.code}: The MCP client returned decline; this does not establish whether a person saw or rejected the prompt. No access was granted. If no prompt appeared, check the client's effective MCP elicitation policy (for Codex, approval_policy.granular.mcp_elicitations; Desktop Full Access can override file settings with never, while Custom uses the configured policy). Do not enable Full Access, retry automatically, or reload the browser extension to resolve this response.`;
    }
    if (code === 'MEDIA_AUTHORIZATION_TIMEOUT') {
      error.message = `${error.code}: The confirmation or its request deadline expired. No observation was started; a late answer cannot grant access. Do not retry automatically.`;
    }
    if (code === 'MEDIA_CONSENT_WAIT_UPGRADE_REQUIRED') {
      error.message = `${error.code}: This Broker and Extension must support consent_wait_version 1 before first-page confirmation. Update both together; no prompt or capture was started. Do not retry against the same old runtime.`;
    }
    if (code === 'MEDIA_CONSENT_DURATION_UPGRADE_REQUIRED') {
      error.message = `${error.code}: Three-hour visual consent requires the updated MCP adapter, Broker and Extension together. No prompt or capture was started.`;
    }
    if (code === 'MEDIA_SESSION_CONSENT_UPGRADE_REQUIRED') {
      error.message = `${error.code}: Unified session observation consent requires version 2 in the MCP adapter, Broker and Extension together. No prompt or capture was started.`;
    }
    return error;
  };
  let capabilities = {};
  // The user's decision lasts until this live session closes. Exact-document
  // execution grants still expire and are freshly validated without prompting.
  const pending = new Map();
  const { authorizing, followers } = sessionConsent;
  let closed = false;
  const supportsForm = () => {
    const value = capabilities.elicitation;
    return Boolean(value && typeof value === 'object' && (Object.keys(value).length === 0 || value.form));
  };
  const left = (deadline) => {
    const ms = deadline - now();
    if (ms <= 0) throw failure('MEDIA_AUTHORIZATION_TIMEOUT');
    return ms;
  };
  function abandon(id, code) {
    const item = pending.get(id);
    if (!item) return;
    pending.delete(id); clearTimeout(item.timer);
    // Cancel the server-originated elicitation, not the parent tools/call.
    // Clients may race or ignore this notification; late responses are still inert.
    output.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: id, reason: code } })}\n`);
    item.reject(failure(code));
  }
  function ask(params, requestId, deadline, phased = false) {
    if (!supportsForm()) throw failure('MEDIA_CLIENT_CONFIRMATION_UNSUPPORTED');
    const id = `saccade-consent:${randomUUID()}`;
    return new Promise((resolve, reject) => {
      const wait = Math.min(left(deadline), confirmationTimeoutMs);
      // Infinity is a live-session wait, not a Node timer (which would fire in 1ms).
      const timer = Number.isFinite(wait) ? setTimeout(() => abandon(id, 'MEDIA_AUTHORIZATION_TIMEOUT'), wait) : undefined;
      pending.set(id, { requestId, timer, resolve, reject, expires: Math.min(deadline, now() + wait) });
      output.write(`${JSON.stringify({ jsonrpc: '2.0', id, method: 'elicitation/create', params })}\n`);
    });
  }
  const api = {
    initialize(value) { capabilities = value || {}; },
    describe() { return { version: 1, consent_wait_version: 1, host_intent_attestation: capabilities.experimental?.[intentKey] === 1, form_confirmation: supportsForm(),
      session_consent_version: 2, grant_scope: 'session_observation', consent_lifetime: 'live_session',
      observations: ['visual', 'media'], max_document_grant_ms: VISUAL_GRANT_MS,
      consent_active: Boolean(sessionConsent.decision && !sessionConsent.closed) }; },
    receive(message) {
      if (message.method) return false;
      if (!pending.has(message.id)) return false;
      if (now() >= pending.get(message.id).expires) {
        abandon(message.id, 'MEDIA_AUTHORIZATION_TIMEOUT');
        return true;
      }
      const item = pending.get(message.id); pending.delete(message.id); clearTimeout(item.timer);
      if (message.error) {
        // Preserve the protocol distinction without reflecting arbitrary client
        // messages, which may contain sensitive request data.
        const code = message.error.code;
        item.reject(failure(code === -32601 ? 'MEDIA_CLIENT_CONFIRMATION_UNSUPPORTED'
          : code === -32602 ? 'MEDIA_CLIENT_CONFIRMATION_INVALID_REQUEST'
            : 'MEDIA_CLIENT_CONFIRMATION_FAILED'));
      } else item.resolve(message.result);
      return true;
    },
    cancel(requestId) {
      for (const item of followers) if (item.requestId === requestId) item.finish(failure('MEDIA_AUTHORIZATION_CANCELLED'));
      for (const [id, item] of pending) if (item.requestId === requestId) {
        abandon(id, 'MEDIA_AUTHORIZATION_CANCELLED');
      }
    },
    close() {
      closed = true;
      sessionConsent.closed = true;
      sessionConsent.decision = undefined;
      for (const item of followers) item.finish(failure('MEDIA_AUTHORIZATION_CANCELLED'));
      for (const id of pending.keys()) abandon(id, 'MEDIA_AUTHORIZATION_CANCELLED');
    },
    async ensure(request, deadline, options = {}) {
      const phased = Number.isSafeInteger(options.executionTimeoutMs) && options.executionTimeoutMs > 0 && options.executionTimeoutMs <= 30000;
      const gateDeadline = options.gateDeadline ?? Infinity;
      const args = request.params.arguments;
      const scope = { tab_id: args.tab_id, document_id: args.document_id };
      const intent = request.params._meta?.[intentKey];
      const attested = capabilities.experimental?.[intentKey] === 1 && intent
        && ['explicit_request', 'confirmation'].includes(intent.kind)
        && intent.tab_id === scope.tab_id && intent.document_id === scope.document_id;
      // A host-attested request for one page is not session-wide permission.
      const sessionScoped = !attested;
      const reusedConsent = sessionScoped ? sessionConsent.decision : undefined;
      const sceneScope = observation === 'visual' && args.object_id && args.scene_generation
        ? { scene_object_id: args.object_id, scene_generation: args.scene_generation,
          ...(args.mode !== 'sequence' ? { scene_mode: 'snapshot' } : {}) } : {};
      const check = () => { left(deadline); if (closed || sessionConsent.closed || isCancelled(request.id)) throw failure('MEDIA_AUTHORIZATION_CANCELLED'); };
      check();
      let prepared;
      try {
        prepared = await invoke(`${observation}.authorization.prepare`, { ...scope, ...sceneScope,
          ...(sessionScoped ? { grant_ttl_ms: VISUAL_GRANT_MS, grant_scope: 'session_observation' }
            : observation === 'visual' ? { grant_ttl_ms: VISUAL_GRANT_MS } : {}),
          ...(phased ? { wait_for_consent: true } : {}) }, left(deadline), request.id, deadline);
      } catch (error) {
        if (sessionScoped && (error.code === 'INVALID_REQUEST' || error.code?.endsWith('SESSION_CONSENT_UPGRADE_REQUIRED'))) throw failure('MEDIA_SESSION_CONSENT_UPGRADE_REQUIRED');
        if (observation === 'visual' && error.code === 'INVALID_REQUEST') throw failure('MEDIA_CONSENT_DURATION_UPGRADE_REQUIRED');
        if (phased && (error.code === 'INVALID_REQUEST' || error.code?.endsWith('CONSENT_WAIT_UPGRADE_REQUIRED'))) throw failure('MEDIA_CONSENT_WAIT_UPGRADE_REQUIRED');
        throw error;
      }
      check();
      if (prepared.granted === true && prepared.access_kind === 'owner_scene_policy') return { deadline_at: deadline };
      if (sessionScoped && prepared.session_consent_version !== 2) throw failure('MEDIA_SESSION_CONSENT_UPGRADE_REQUIRED');
      if (prepared.granted === true) return { deadline_at: deadline };
      if ((sessionScoped || observation === 'visual') && prepared.grant_ttl_ms !== VISUAL_GRANT_MS) throw failure('MEDIA_CONSENT_DURATION_UPGRADE_REQUIRED');
      if (sessionScoped && prepared.grant_scope !== 'session_observation') throw failure('MEDIA_SESSION_CONSENT_UPGRADE_REQUIRED');
      if (typeof prepared.challenge !== 'string') throw failure('MEDIA_AUTHORIZATION_INVALID_RESPONSE');
      if (phased && (prepared.consent_wait_version !== 1 || !Number.isSafeInteger(prepared.consent_expires_at))) throw failure('MEDIA_CONSENT_WAIT_UPGRADE_REQUIRED');
      let source;
      if (reusedConsent) {
        if (reusedConsent !== sessionConsent.decision) throw failure('MEDIA_AUTHORIZATION_STALE');
        source = 'session_confirmation';
      } else if (attested) source = intent.kind;
      else {
        // A short-lived document challenge must not time out the human decision.
        const confirmationDeadline = phased ? gateDeadline : deadline;
        const answer = await ask({
          message: 'Allow this Agent to see its authorized tabs for this session?\nIncludes screenshots, video frames and captions, and short 3D motion sequences in tabs opened by or explicitly shared with this Agent. Video reading may temporarily load up to 64 MiB and seek or pause playback; no audio transcription or persistent recording. Page screenshots mask input fields and embedded frames, but other text and images may contain personal information. One approval covers new pages, refreshes and verified reconnects until this session ends. Other Agents and unshared tabs are excluded. Stop sharing revokes a tab; the screenshot and video Off controls block that access.'
            + ' Nothing is captured while waiting. You only need to approve once for this session.',
          // The client's Accept / Decline is the decision. A second, unchecked
          // boolean made Accept look successful while still denying access.
          requestedSchema: { type: 'object', properties: {} },
        }, request.id, confirmationDeadline, phased);
        // A client may decline without displaying UI. Do not attribute this to a person.
        if (answer?.action === 'decline') throw failure('MEDIA_CLIENT_CONFIRMATION_DECLINED');
        if (answer?.action === 'cancel') throw failure('MEDIA_AUTHORIZATION_CANCELLED');
        if (answer?.action !== 'accept' || !answer.content || typeof answer.content !== 'object' || Array.isArray(answer.content)) throw failure('MEDIA_AUTHORIZATION_INVALID_RESPONSE');
        // Accept only the empty confirmation form, or an explicit boolean from
        // older compatible clients. Never turn false or arbitrary data into Yes.
        const keys = Object.keys(answer.content);
        if (keys.length && (keys.length !== 1 || keys[0] !== 'allow' || typeof answer.content.allow !== 'boolean')) throw failure('MEDIA_AUTHORIZATION_INVALID_RESPONSE');
        if (answer.content.allow === false) throw failure('MEDIA_AUTHORIZATION_NOT_GRANTED');
        if (phased) {
          // The pre-consent gate performed no observation. Allocate the single
          // acquisition deadline only after a valid answer, then pass it intact.
          left(confirmationDeadline);
          deadline = now() + options.executionTimeoutMs;
        }
        check();
        // Consent is the human decision, not the success of this page's grant.
        // A stale page or failed capture must not discard it and ask again.
        if (sessionScoped) sessionConsent.decision = {};
        if (phased && (sessionScoped || now() >= prepared.consent_expires_at)) {
          // Human waiting can outlive a connection/activation epoch even when
          // the challenge's timer has not expired. Derive the first document
          // grant from the saved decision just like a coalesced follower does.
          // Revalidate the SAME document once under the acquisition deadline;
          // never accept the pre-wait challenge or replay capture after failure.
          return authorize(request, deadline, options);
        }
        source = 'client_confirmation';
      }
      check();
      const accepted = await invoke(`${observation}.authorization.accept`, { ...scope, challenge: prepared.challenge, source }, left(deadline), request.id, deadline);
      if (accepted?.granted !== true) throw failure('MEDIA_AUTHORIZATION_INVALID_RESPONSE');
      check();
      if (sessionScoped) {
        if (accepted.session_consent_version !== 2 || accepted.grant_scope !== 'session_observation'
          || !Number.isSafeInteger(accepted.expires_at) || accepted.expires_at <= now()
          || accepted.expires_at > now() + VISUAL_GRANT_MS) throw failure('MEDIA_AUTHORIZATION_INVALID_RESPONSE');
      }
      return { deadline_at: deadline };
    },
  };
  const authorize = api.ensure;
  api.ensure = async (request, deadline, options = {}) => {
    const phased = Number.isSafeInteger(options.executionTimeoutMs) && options.executionTimeoutMs > 0 && options.executionTimeoutMs <= 30000;
    const gateDeadline = options.gateDeadline ?? Infinity;
    const key = 'session-observation';
    left(deadline);
    if (closed || sessionConsent.closed || isCancelled(request.id)) throw failure('MEDIA_AUTHORIZATION_CANCELLED');
    const active = authorizing.get(key);
    if (active) {
      if (followers.size >= 64) throw failure('MEDIA_AUTHORIZATION_BUSY');
      // Coalesce the decision across tabs and observation kinds, not authority.
      // Each follower prepares its own exact current document after the answer.
      await new Promise((resolve, reject) => {
        let settled = false;
        const item = { requestId: request.id, finish(error) {
          if (settled) return;
          settled = true; clearTimeout(timer); followers.delete(item);
          if (error) reject(error); else resolve();
        } };
        const wait = left(phased ? gateDeadline : deadline);
        const timer = Number.isFinite(wait) ? setTimeout(() => item.finish(failure('MEDIA_AUTHORIZATION_TIMEOUT')), wait) : undefined;
        followers.add(item);
        active.then(() => item.finish(), error => item.finish(error));
      });
      if (phased) { left(gateDeadline); deadline = now() + options.executionTimeoutMs; }
      left(deadline);
      if (closed || sessionConsent.closed || isCancelled(request.id)) throw failure('MEDIA_AUTHORIZATION_CANCELLED');
      return api.ensure(request, deadline, { ...options, gateDeadline });
    }
    if (authorizing.size >= 64) throw failure('MEDIA_AUTHORIZATION_BUSY');
    const task = authorize(request, deadline, { ...options, gateDeadline });
    authorizing.set(key, task);
    try { return await task; }
    finally { if (authorizing.get(key) === task) authorizing.delete(key); }
  };
  return api;
}
module.exports = { createMediaAuthorization, createSessionObservationConsent, INTENT_KEY };
