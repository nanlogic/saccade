importScripts('candidate_identity.js', 'protocol.js', 'consent.js');

const { randomToken } = globalThis.SaccadeProtocol;
const { isSupportedUrl, normalizeOrigin } = globalThis.SaccadeConsent;
const LOADED_CANDIDATE = globalThis.SaccadeCandidate;
const BROKER_ORIGIN = 'http://127.0.0.1:32177';
const BROWSER_FAMILY = navigator.userAgent.includes('Edg/') ? 'edge' : 'chrome';
const INSTANCE_KEY = 'saccade.browser_instance_id';
const TAB_ACL_KEY = 'saccade.tab_acl';
const BROWSER_SESSION_KEY = 'saccade.browser_session_initialized';
const CONNECTION_SESSION_KEY = 'saccade.connection_session_id';
const WORKER_INSTANCE_ID = randomToken('worker');
const KEEPALIVE_INTERVAL_MS = 20_000;
const BROKER_REQUEST_TIMEOUT_MS = 5_000;
const COLLECTOR_MESSAGE_TIMEOUT_MS = 1_000;
const ACTION_RESPONSE_RESERVE_MS = 250;
const CLAIM_TTL_MS = 30_000;
const agentOwnedTabs = new Set();
const userSharedTabs = new Set();
const claimedAgentTabs = new Set();
const sessions = new Map();
const authorizationPromises = new Map();
// Session-only, single-use claim intent. Never persisted: a replaced Service
// Worker must fail closed and force a re-arm.
let pendingClaim;
let browserInstanceId;
let connectionSessionId;
let brokerConnectionId;
let brokerEpoch;
let connectPromise;
let reconnectAttempts = 0;
let reconnectTimer;
let brokerLoopGeneration = 0;
let commandLoopState;
let tabRecoveryState;
let keepaliveSocket;
let keepaliveTimer;
const pendingEvents = [];
let flushPromise;

function sameCandidate(candidate) {
  return candidate?.schema === LOADED_CANDIDATE.schema
    && candidate?.id === LOADED_CANDIDATE.id
    && candidate?.version === LOADED_CANDIDATE.version;
}

async function reloadIfCandidateChanged() {
  try {
    const url = `${chrome.runtime.getURL('candidate.json')}?candidate_check=${Date.now()}`;
    const response = await fetch(url, {
      cache: 'no-store', signal: AbortSignal.timeout(BROKER_REQUEST_TIMEOUT_MS),
    });
    const installed = await response.json();
    if (!sameCandidate(installed)) {
      chrome.runtime.reload();
      throw new Error('activating updated Saccade Extension candidate');
    }
  } catch (error) {
    if (String(error?.message || error).includes('activating updated')) throw error;
    console.error(`Saccade candidate self-check unavailable: ${String(error?.message || error)}`);
  }
}

async function persistAcl() {
  await chrome.storage.local.set({ [TAB_ACL_KEY]: {
    agent: [...agentOwnedTabs], shared: [...userSharedTabs], claimed: [...claimedAgentTabs],
  } });
}

async function initialize() {
  const [identity, storedAcl, browserSession] = await Promise.all([
    chrome.storage.local.get(INSTANCE_KEY),
    chrome.storage.local.get(TAB_ACL_KEY),
    chrome.storage.session.get([BROWSER_SESSION_KEY, CONNECTION_SESSION_KEY]),
  ]);
  browserInstanceId = identity[INSTANCE_KEY] || randomToken('browser');
  if (!identity[INSTANCE_KEY]) await chrome.storage.local.set({ [INSTANCE_KEY]: browserInstanceId });
  connectionSessionId = browserSession[CONNECTION_SESSION_KEY] || randomToken('browser_session');
  const freshBrowserSession = browserSession[BROWSER_SESSION_KEY] !== true;
  if (freshBrowserSession || !browserSession[CONNECTION_SESSION_KEY]) {
    await Promise.all([
      ...(freshBrowserSession ? [chrome.storage.local.remove(TAB_ACL_KEY)] : []),
      chrome.storage.session.set({
        [BROWSER_SESSION_KEY]: true,
        [CONNECTION_SESSION_KEY]: connectionSessionId,
      }),
    ]);
  }
  const acl = freshBrowserSession ? {} : (storedAcl[TAB_ACL_KEY] || {});
  for (const value of acl.agent || []) if (Number.isSafeInteger(value)) agentOwnedTabs.add(value);
  for (const value of acl.shared || []) if (Number.isSafeInteger(value)) userSharedTabs.add(value);
  for (const value of acl.claimed || []) if (Number.isSafeInteger(value) && agentOwnedTabs.has(value)) claimedAgentTabs.add(value);
  for (const tabId of new Set([...agentOwnedTabs, ...userSharedTabs])) {
    try { await chrome.tabs.get(tabId); } catch (_error) { forgetTab(tabId); }
  }
  await persistAcl();
}

function forgetTab(tabId) {
  agentOwnedTabs.delete(tabId);
  userSharedTabs.delete(tabId);
  claimedAgentTabs.delete(tabId);
  if (pendingClaim?.candidates) pendingClaim.candidates.delete(tabId);
  if (pendingClaim?.latchedTabId === tabId) pendingClaim = undefined;
}

function activeClaim() {
  if (!pendingClaim) return null;
  if (Date.now() > pendingClaim.expiresAt) { pendingClaim = undefined; return null; }
  return pendingClaim;
}

function isAuthorized(tabId) { return agentOwnedTabs.has(tabId) || userSharedTabs.has(tabId); }

function brokerRuntimePresent() {
  return Boolean(brokerConnectionId
    && commandLoopState?.connectionId === brokerConnectionId
    && keepaliveSocket?.saccadeConnectionId === brokerConnectionId
    && keepaliveSocket.readyState !== WebSocket.CLOSING
    && keepaliveSocket.readyState !== WebSocket.CLOSED);
}

function brokerRuntimeReady() {
  return brokerRuntimePresent() && keepaliveSocket.readyState === WebSocket.OPEN;
}

async function ensureBrokerConnection() {
  if (brokerRuntimePresent() || connectPromise) return;
  if (brokerConnectionId) {
    const connectionId = brokerConnectionId;
    brokerConnectionId = undefined;
    brokerEpoch = undefined;
    pendingClaim = undefined;
    brokerLoopGeneration += 1;
    commandLoopState = undefined;
    stopKeepalive(connectionId);
  }
  await connectBroker();
}

async function tabStatus(tabId) {
  const tab = await chrome.tabs.get(tabId);
  const supported = isSupportedUrl(tab.url);
  const session = sessions.get(tabId);
  return {
    tab_id: String(tabId), supported, agent_owned: agentOwnedTabs.has(tabId),
    shared: userSharedTabs.has(tabId), authorized: isAuthorized(tabId),
    provenance: tabProvenance(tabId),
    observation_ready: Boolean(session?.observationReady), collector_error: session?.error,
    broker_connected: brokerRuntimeReady(),
  };
}

function tabProvenance(tabId) {
  if (claimedAgentTabs.has(tabId)) return 'agent_client';
  if (agentOwnedTabs.has(tabId)) return 'saccade_tabs_open';
  if (userSharedTabs.has(tabId)) return 'user_shared';
  return 'none';
}

async function revokeTabAccess(tabId) {
  forgetTab(tabId);
  sessions.delete(tabId);
  await persistAcl();
  try { await collectorMessage(tabId, { kind: 'collector.deauthorize' }); } catch (_error) { /* already gone */ }
}

const RECONNECT_ALARM = 'saccade.node-broker-reconnect';
const RECONNECT_ALARM_PERIOD_MINUTES = 0.5;

function armReconnectAlarm() {
  chrome.alarms.create(RECONNECT_ALARM, { periodInMinutes: RECONNECT_ALARM_PERIOD_MINUTES });
}

function scheduleReconnect(error) {
  if (error) console.error(`Saccade reconnect scheduled: ${String(error.message || error)}`);
  if (brokerConnectionId) return;
  if (reconnectTimer) {
    armReconnectAlarm();
    return;
  }
  const delay = Math.min(250 * (2 ** reconnectAttempts++), 4000);
  armReconnectAlarm();
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    if (brokerConnectionId) return;
    if (connectPromise) {
      const pending = connectPromise;
      pending.finally(() => {
        if (!brokerConnectionId && connectPromise !== pending) scheduleReconnect();
      });
      return;
    }
    connectBroker().catch(scheduleReconnect);
  }, delay);
}

async function reconnectAfterWindowRemoval() {
  armReconnectAlarm();
  if (brokerConnectionId) return;
  try { await connectBroker(); } catch (_error) { scheduleReconnect(); }
}

async function brokerRequest(path, options = {}) {
  const { timeoutMs = BROKER_REQUEST_TIMEOUT_MS, ...fetchOptions } = options;
  const response = await fetch(`${BROKER_ORIGIN}${path}`, {
    cache: 'no-store',
    ...fetchOptions,
    signal: fetchOptions.signal || AbortSignal.timeout(timeoutMs),
    headers: { 'content-type': 'application/json', ...(options.headers || {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body?.error?.message || `Broker returned HTTP ${response.status}`);
  return body;
}

async function settleReconnect(connectionId) {
  if (brokerConnectionId !== connectionId || !brokerRuntimeReady()) return;
  reconnectAttempts = 0;
  armReconnectAlarm();
}

function boundedPromise(promise, timeoutMs, message) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      const error = new Error(message);
      error.saccadeLocalTimeout = true;
      reject(error);
    }, Math.max(1, timeoutMs));
  });
  return Promise.race([Promise.resolve(promise), timeout]).finally(() => clearTimeout(timer));
}

function collectorMessage(tabId, message, timeoutMs = COLLECTOR_MESSAGE_TIMEOUT_MS) {
  return boundedPromise(
    chrome.tabs.sendMessage(tabId, message, { frameId: 0 }),
    timeoutMs,
    'Collector message timed out',
  );
}

async function flushEvents(connectionId = brokerConnectionId) {
  if (flushPromise) return flushPromise;
  flushPromise = (async () => {
    while (connectionId && connectionId === brokerConnectionId && pendingEvents.length) {
      const events = pendingEvents.splice(0, 128);
      try {
        await brokerRequest('/v1/extension/events', {
          method: 'POST', body: JSON.stringify({ connection_id: connectionId, events }),
        });
      } catch (error) {
        // Responses from an old connection must never be replayed into a new epoch.
        for (const event of events.reverse()) if (event.kind !== 'response') pendingEvents.unshift(event);
        if (brokerConnectionId === connectionId) brokerConnectionId = undefined;
        brokerEpoch = undefined;
        throw error;
      }
    }
  })().finally(() => {
    flushPromise = undefined;
    if (brokerConnectionId && pendingEvents.length) queueMicrotask(() => flushEvents().catch(scheduleReconnect));
  });
  return flushPromise;
}

function queueEvent(event) {
  pendingEvents.push(event);
  flushEvents().catch(scheduleReconnect);
}

function stopKeepalive(connectionId) {
  if (!keepaliveSocket || (connectionId && keepaliveSocket.saccadeConnectionId !== connectionId)) return;
  const socket = keepaliveSocket;
  keepaliveSocket = undefined;
  if (keepaliveTimer) { clearInterval(keepaliveTimer); keepaliveTimer = undefined; }
  socket.onclose = null;
  socket.onerror = null;
  try { socket.close(1000, 'connection reset'); } catch (_error) { /* already closed */ }
}

function startKeepalive(connectionId) {
  stopKeepalive();
  const socket = new WebSocket(
    `ws://127.0.0.1:32177/v1/extension/keepalive?connection_id=${encodeURIComponent(connectionId)}`,
  );
  socket.saccadeConnectionId = connectionId;
  keepaliveSocket = socket;
  const heartbeat = () => {
    if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ kind: 'heartbeat' }));
  };
  socket.onopen = () => {
    if (keepaliveSocket !== socket || brokerConnectionId !== connectionId) return socket.close();
    heartbeat();
    keepaliveTimer = setInterval(heartbeat, KEEPALIVE_INTERVAL_MS);
    settleReconnect(connectionId).catch(scheduleReconnect);
    return undefined;
  };
  socket.onmessage = (event) => {
    let message;
    try { message = JSON.parse(String(event.data)); } catch (_error) { return socket.close(); }
    if (message?.kind !== 'heartbeat.ack' || message.broker_epoch !== brokerEpoch) socket.close();
    return undefined;
  };
  socket.onerror = () => socket.close();
  socket.onclose = () => {
    if (keepaliveSocket !== socket) return;
    keepaliveSocket = undefined;
    if (keepaliveTimer) { clearInterval(keepaliveTimer); keepaliveTimer = undefined; }
    if (brokerConnectionId !== connectionId) return;
    brokerConnectionId = undefined;
    brokerEpoch = undefined;
    pendingClaim = undefined;
    scheduleReconnect(new Error('Broker keepalive closed'));
  };
}

async function commandLoop(connectionId, generation) {
  while (brokerConnectionId === connectionId && brokerLoopGeneration === generation) {
    const body = await brokerRequest('/v1/extension/commands', {
      method: 'POST', body: JSON.stringify({ connection_id: connectionId }),
    });
    for (const command of body.commands || []) {
      try {
        await handleHostCommand(command);
      } catch (error) {
        queueEvent({ kind: 'response', command_id: command.command_id, error: {
          code: error.saccadeCode || 'EXTENSION_REJECTED',
          message: String(error?.message || error).slice(0, 512),
          stage: error.saccadeStage || 'extension',
          outcome: error.saccadeOutcome,
          retry_safe: error.saccadeRetrySafe === true,
        } });
      }
    }
    await flushEvents(connectionId);
  }
}

function startCommandLoop(connectionId, generation) {
  const state = { connectionId, generation, promise: null };
  state.promise = commandLoop(connectionId, generation).catch((error) => {
    if (commandLoopState !== state
        || brokerConnectionId !== connectionId
        || brokerLoopGeneration !== generation) return;
    brokerConnectionId = undefined;
    brokerEpoch = undefined;
    pendingClaim = undefined;
    stopKeepalive(connectionId);
    scheduleReconnect(error);
  }).finally(() => {
    if (commandLoopState === state) commandLoopState = undefined;
  });
  commandLoopState = state;
}

function startTabRecovery(connectionId, requireFullTruth) {
  const state = { connectionId, promise: null };
  const tabIds = [...new Set([...agentOwnedTabs, ...userSharedTabs])];
  state.promise = Promise.allSettled(tabIds.map(async (tabId) => {
    if (brokerConnectionId !== connectionId) return;
    try {
      await authorizeTab(tabId);
      if (requireFullTruth && brokerConnectionId === connectionId) {
        await requestCollectorSnapshot(tabId);
      }
    } catch (error) { reportAuthorizationFailure(error); }
  })).finally(() => {
    if (tabRecoveryState === state) tabRecoveryState = undefined;
  });
  tabRecoveryState = state;
}

async function connectBroker() {
  if (brokerConnectionId) return;
  if (connectPromise) return connectPromise;
  connectPromise = (async () => {
    await reloadIfCandidateChanged();
    if (!browserInstanceId) await initialize();
    const connected = await brokerRequest('/v1/extension/connect', {
      method: 'POST', body: JSON.stringify({
      browser_instance_id: browserInstanceId,
      browser_session_id: connectionSessionId,
      worker_instance_id: WORKER_INSTANCE_ID,
      extension_candidate: LOADED_CANDIDATE,
      browser_family: BROWSER_FAMILY,
      authorized_tabs: [...new Set([...agentOwnedTabs, ...userSharedTabs])].map((tabId) => ({
        tab_id: String(tabId), provenance: tabProvenance(tabId),
      })),
      }),
    });
    brokerConnectionId = connected.connection_id;
    brokerEpoch = connected.broker_epoch;
    try {
      startKeepalive(connected.connection_id);
    } catch (error) {
      brokerConnectionId = undefined;
      brokerEpoch = undefined;
      throw error;
    }
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = undefined; }
    pendingEvents.length = 0;
    const generation = ++brokerLoopGeneration;
    startCommandLoop(connected.connection_id, generation);
    startTabRecovery(connected.connection_id, connected.require_full_truth);
  })().finally(() => { connectPromise = undefined; });
  return connectPromise;
}

function numericTabId(value) {
  const id = Number(value);
  if (!Number.isSafeInteger(id) || id < 0) throw new Error('tab_id must identify a browser tab');
  return id;
}

function navigableUrl(value) {
  const url = new URL(String(value || ''));
  if (!['http:', 'https:'].includes(url.protocol) || url.href.length > 8192) throw new Error('URL must use HTTP or HTTPS');
  return url.href;
}

async function openAgentTab(url, active) {
  const normalWindows = await chrome.windows.getAll({ windowTypes: ['normal'] });
  const targetWindow = normalWindows.find((window) => window.focused) || normalWindows.at(-1);
  if (targetWindow?.id !== undefined && targetWindow.id !== chrome.windows.WINDOW_ID_NONE) {
    return chrome.tabs.create({
      windowId: targetWindow.id, url, active,
    });
  }

  const createdWindow = await chrome.windows.create({
    url, type: 'normal', focused: active,
  });
  let tab = createdWindow.tabs?.[0];
  if (!tab && createdWindow.id !== undefined) {
    [tab] = await chrome.tabs.query({ windowId: createdWindow.id, active: true });
  }
  if (!tab) throw new Error('browser did not return a tab for the new window');
  return tab;
}

// Step 1: arm one session-only intent bound to the requested origin. No tab is
// created, opened, or authorized here, and any earlier unconsumed claim dies.
function armTabClaim(url) {
  const origin = normalizeOrigin(navigableUrl(url));
  pendingClaim = {
    claimId: randomToken('claim'),
    origin,
    expiresAt: Date.now() + CLAIM_TTL_MS,
    latchedTabId: null,
    candidates: new Set(),
  };
  return { claim: 'armed', claim_id: pendingClaim.claimId, origin, expires_in_ms: CLAIM_TTL_MS };
}

// Step 2 (passive): the Agent creates the tab with its own browser tooling.
// Only a tab created after the claim was armed may become a candidate, only the
// event payload for that tab is inspected, and no tab is enumerated, read, or
// authorized here. Latching records which single tab a later confirm may name.
function noteClaimCandidate(tab) {
  const claim = activeClaim();
  if (!claim || claim.latchedTabId !== null) return;
  if (tab?.id === undefined || !Number.isSafeInteger(tab.id)) return;
  if (isAuthorized(tab.id)) return;
  claim.candidates.add(tab.id);
  considerClaimCandidate(tab.id, tab.pendingUrl || tab.url);
}

function considerClaimCandidate(tabId, url) {
  const claim = activeClaim();
  if (!claim || claim.latchedTabId !== null || !claim.candidates.has(tabId)) return;
  // A tab an Agent client just created sits on a browser-internal page such as
  // chrome://newtab/, about:blank, or no URL at all, and only reaches its real
  // destination on a later navigation. None of those is a settled URL, so the
  // candidate waits rather than spending its one decision on them.
  if (!isSupportedUrl(url)) return;
  claim.candidates.delete(tabId); // one decision per new tab; never reconsidered
  if (normalizeOrigin(url) !== claim.origin) return;
  claim.latchedTabId = tabId;
  claim.candidates.clear(); // first qualifying tab wins; no second candidate
}

// Step 3: confirm. Every mismatch consumes the single-use claim and returns one
// generic failure so a caller cannot probe tab identities or claim state.
async function confirmTabClaim(payload) {
  const claim = activeClaim();
  pendingClaim = undefined;
  const rejected = new Error('tab claim could not be confirmed');
  if (!claim || claim.latchedTabId === null) throw rejected;
  if (String(payload.claim_id || '') !== claim.claimId) throw rejected;
  let requestedTabId;
  try {
    requestedTabId = numericTabId(payload.tab_id);
    if (normalizeOrigin(navigableUrl(payload.url)) !== claim.origin) throw rejected;
  } catch (_error) { throw rejected; }
  if (requestedTabId !== claim.latchedTabId) throw rejected;
  if (userSharedTabs.has(requestedTabId)) throw rejected;
  let tab;
  try { tab = await chrome.tabs.get(requestedTabId); } catch (_error) { throw rejected; }
  if (!isSupportedUrl(tab.url) || normalizeOrigin(tab.url) !== claim.origin) throw rejected;
  agentOwnedTabs.add(requestedTabId);
  claimedAgentTabs.add(requestedTabId);
  await persistAcl();
  return { tab_id: String(requestedTabId), claim: 'confirmed', opened: false, provenance: 'agent_client' };
}

async function authorizeTab(tabId, { recoverStale = false } = {}) {
  const tab = await chrome.tabs.get(tabId);
  if (!isSupportedUrl(tab.url)) throw new Error('tab URL is not supported');
  const existing = authorizationPromises.get(tabId);
  if (existing?.url === tab.url) {
    return existing.promise.catch(() => {}).then(() => {
      const session = sessions.get(tabId);
      if (session?.url === tab.url && (session.configuring || session.configured)) return;
      return authorizeTab(tabId, { recoverStale });
    });
  }
  if (existing) {
    return existing.promise.catch(() => {}).then(() => authorizeTab(tabId, { recoverStale }));
  }
  const entry = { url: tab.url, promise: null };
  entry.promise = authorizeTabInner(tabId, tab.url, recoverStale).finally(() => {
    if (authorizationPromises.get(tabId) === entry) authorizationPromises.delete(tabId);
  });
  authorizationPromises.set(tabId, entry);
  return entry.promise;
}

async function waitForCurrentCollector(tabId, attempts) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const ping = await collectorMessage(tabId, { kind: 'collector.ping' }, 100);
      if (ping?.ok === true && sameCandidate(ping.extension_candidate)) return true;
    } catch (_error) { /* static bundle may still be starting */ }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return false;
}

async function authorizeTabInner(tabId, expectedUrl, recoverStale) {
  if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
  const tab = await chrome.tabs.get(tabId);
  if (tab.url !== expectedUrl) throw new Error('tab URL changed during collector authorization');
  if (!isSupportedUrl(tab.url)) throw new Error('tab URL is not supported');
  const prior = sessions.get(tabId);
  if (prior?.url === tab.url && (prior.configuring || prior.configured)) return;
  sessions.set(tabId, { observationReady: false, url: tab.url, configuring: true });
  let ready = await waitForCurrentCollector(tabId, 40);
  if (!ready && recoverStale) {
    await chrome.tabs.reload(tabId);
    ready = await waitForCurrentCollector(tabId, 200);
  }
  if (!ready) throw new Error('static Collector is unavailable or stale');
  try {
    const configured = await collectorMessage(tabId, { kind: 'collector.configure', config: {
      browserInstanceId, tabId: String(tabId), frameId: `frame.${tabId}.0`,
    } });
    if (!configured?.ok) throw new Error(configured?.error || 'collector configuration failed');
    const session = sessions.get(tabId);
    if (session?.url === tab.url) { session.configuring = false; session.configured = true; }
  } catch (error) {
    const detail = String(error?.message || error).replace(/[\r\n]+/g, ' ').slice(0, 512);
    sessions.set(tabId, { observationReady: false, url: tab.url, error: detail });
    throw error;
  }
}

function reportAuthorizationFailure(error) {
  console.error(`Saccade collector authorization failed: ${String(error?.message || error)}`);
}

function reply(command, payload) {
  if (command.command_id) queueEvent({ kind: 'response', command_id: command.command_id, result: payload });
}

function extensionDeadlineError(message) {
  const error = new Error(message);
  error.saccadeStage = 'extension_queue';
  error.saccadeCode = 'deadline_exceeded';
  error.saccadeRetrySafe = true;
  return error;
}

function extensionOutcomeUnknownError(message) {
  const error = new Error(message);
  error.saccadeStage = 'dispatch';
  error.saccadeCode = 'OUTCOME_UNKNOWN';
  error.saccadeOutcome = 'outcome_unknown';
  error.saccadeRetrySafe = false;
  return error;
}

async function collectorCommand(tabId, message, timeoutMs, { sideEffect = true } = {}) {
  try {
    return await collectorMessage(tabId, message, timeoutMs);
  } catch (error) {
    if (error?.saccadeLocalTimeout) {
      if (sideEffect) {
        throw extensionOutcomeUnknownError(
          'Collector response did not arrive after action dispatch; the outcome is unknown',
        );
      }
      throw extensionDeadlineError('Collector response did not arrive before the command deadline');
    }
    throw error;
  }
}

function remainingCommandMs(deadlineAt) {
  return Math.min(30_000, Number(deadlineAt) - Date.now());
}

function scrubUploadPayload(payload) {
  if (payload?.operation === 'upload' && payload.payload?.file) {
    delete payload.payload.file.content_base64;
  }
}

async function activateTabForAction(tabId, deadlineAt) {
  const tab = await chrome.tabs.get(tabId);
  const browserWindow = await chrome.windows.get(tab.windowId);
  let focusChanged = false;
  if (!browserWindow.focused) {
    await chrome.windows.update(tab.windowId, { focused: true });
    focusChanged = true;
  }
  if (!tab.active) {
    await chrome.tabs.update(tabId, { active: true });
    focusChanged = true;
  }
  let remainingMs = remainingCommandMs(deadlineAt);
  if (remainingMs <= ACTION_RESPONSE_RESERVE_MS) {
    throw extensionDeadlineError('command deadline elapsed during tab activation');
  }
  if (focusChanged) {
    await new Promise((resolve) => setTimeout(
      resolve, Math.min(100, remainingMs - ACTION_RESPONSE_RESERVE_MS),
    ));
    remainingMs = remainingCommandMs(deadlineAt);
  }
  if (remainingMs <= ACTION_RESPONSE_RESERVE_MS) {
    throw extensionDeadlineError('command deadline elapsed before Collector dispatch');
  }
  return remainingMs - ACTION_RESPONSE_RESERVE_MS;
}

async function handleHostCommand(command) {
  const remainingMs = remainingCommandMs(command.deadline_at);
  if (!Number.isFinite(remainingMs) || remainingMs <= 0) {
    throw extensionDeadlineError('command deadline elapsed before Extension dispatch');
  }
  const payload = {
    ...command.payload,
    timeout_ms: Math.min(Number(command.payload?.timeout_ms) || remainingMs, remainingMs),
  };
  if (Array.isArray(payload.steps)) {
    payload.steps = payload.steps.map((step) => ({ ...step, timeout_ms: payload.timeout_ms }));
  }
  if (command.kind === 'tabs.list') {
    const tabs = [];
    for (const tabId of new Set([...agentOwnedTabs, ...userSharedTabs])) {
      try {
        const tab = await chrome.tabs.get(tabId);
        if (!isSupportedUrl(tab.url)) continue;
        const session = sessions.get(tabId);
        const item = {
          tab_id: String(tabId), title: tab.title || '', url: tab.url || '',
          active: Boolean(tab.active), observation_ready: Boolean(session?.observationReady),
          ownership: agentOwnedTabs.has(tabId) ? 'agent' : 'user_shared',
          provenance: tabProvenance(tabId),
        };
        if (session?.error) item.collector_error = session.error;
        tabs.push(item);
      } catch (_error) { forgetTab(tabId); }
    }
    await persistAcl();
    reply(command, { tabs });
  } else if (command.kind === 'tabs.open' && payload.claim === 'arm') {
    reply(command, armTabClaim(payload.url));
  } else if (command.kind === 'tabs.open' && payload.claim === 'confirm') {
    const claimed = await confirmTabClaim(payload);
    reply(command, { ...claimed, browser_instance_id: browserInstanceId });
    authorizeTab(numericTabId(claimed.tab_id)).catch(reportAuthorizationFailure);
  } else if (command.kind === 'tabs.open' && payload.claim === 'shared') {
    const tabId = numericTabId(payload.tab_id);
    if (!userSharedTabs.has(tabId)) throw new Error('tab is not explicitly user-shared');
    const tab = await chrome.tabs.get(tabId);
    if (!isSupportedUrl(tab.url)) throw new Error('tab URL is not supported');
    reply(command, { tab_id: String(tabId), opened: false, provenance: 'user_shared', browser_instance_id: browserInstanceId });
    authorizeTab(tabId).catch(reportAuthorizationFailure);
  } else if (command.kind === 'tabs.open') {
    if (payload.claim !== undefined) throw new Error('claim must be arm or confirm');
    const tab = await openAgentTab(navigableUrl(payload.url), payload.active !== false);
    if (tab.id === undefined) throw new Error('browser did not return a tab identity');
    agentOwnedTabs.add(tab.id);
    await persistAcl();
    reply(command, { tab_id: String(tab.id), opened: true, browser_instance_id: browserInstanceId });
    const current = await chrome.tabs.get(tab.id);
    if (isSupportedUrl(current.url)) authorizeTab(tab.id).catch(reportAuthorizationFailure);
  } else if (command.kind === 'tabs.close') {
    const tabId = numericTabId(payload.tab_id);
    if (!agentOwnedTabs.has(tabId)) throw new Error('only Agent-owned tabs may be closed through Saccade');
    const tab = await chrome.tabs.get(tabId);
    const windowTabs = await chrome.tabs.query({ windowId: tab.windowId });
    const closesLastWindowTab = windowTabs.length === 1;
    if (closesLastWindowTab) {
      sessions.delete(tabId);
      agentOwnedTabs.delete(tabId);
      userSharedTabs.delete(tabId);
      await persistAcl();
      reply(command, { tab_id: String(tabId), closed: true });
      try { await chrome.tabs.remove(tabId); } catch (error) {
        console.error(`Agent-owned tab removal failed after revocation: ${String(error?.message || error)}`);
      }
      return;
    }
    await chrome.tabs.remove(tabId);
    sessions.delete(tabId);
    agentOwnedTabs.delete(tabId);
    userSharedTabs.delete(tabId);
    await persistAcl();
    reply(command, { tab_id: String(tabId), closed: true });
  } else if (command.kind === 'prepare_action') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    payload.timeout_ms = Math.min(
      payload.timeout_ms, await activateTabForAction(tabId, command.deadline_at),
    );
    const result = await collectorCommand(
      tabId, { kind: 'collector.prepare_action', request: payload }, payload.timeout_ms,
      { sideEffect: false },
    );
    if (!result?.ok) throw collectorActionError(result, 'action preparation failed');
    reply(command, result.prepared);
  } else if (command.kind === 'soft_click') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    payload.timeout_ms = Math.min(
      payload.timeout_ms, await activateTabForAction(tabId, command.deadline_at),
    );
    const result = await collectorCommand(
      tabId, { kind: 'collector.soft_click', request: payload }, payload.timeout_ms,
    );
    if (!result?.ok) throw collectorActionError(result, 'soft click failed');
    reply(command, result.result);
  } else if (command.kind === 'soft_action') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    payload.timeout_ms = Math.min(
      payload.timeout_ms, await activateTabForAction(tabId, command.deadline_at),
    );
    const result = await collectorCommand(
      tabId, { kind: 'collector.soft_action', request: payload }, payload.timeout_ms,
    );
    if (!result?.ok) throw collectorActionError(result, 'software action failed');
    reply(command, result.result);
  } else if (command.kind === 'act') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    payload.timeout_ms = Math.min(
      payload.timeout_ms, await activateTabForAction(tabId, command.deadline_at),
    );
    let result;
    try {
      result = await collectorCommand(
        tabId, { kind: 'collector.soft_action', request: payload }, payload.timeout_ms,
      );
    } finally {
      scrubUploadPayload(payload);
    }
    if (!result?.ok) throw collectorActionError(result, 'software action failed');
    reply(command, result.result || { accepted: true });
  } else if (command.kind === 'act.batch') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    payload.timeout_ms = Math.min(
      payload.timeout_ms, await activateTabForAction(tabId, command.deadline_at),
    );
    payload.steps = payload.steps.map((step) => ({ ...step, timeout_ms: payload.timeout_ms }));
    const result = await collectorCommand(
      tabId, { kind: 'collector.soft_action_batch', request: payload }, payload.timeout_ms,
    );
    if (!result?.ok) throw collectorActionError(result, 'form batch failed');
    reply(command, result.result || { accepted: true, steps: [] });
  } else if (command.kind === 'observation.resync') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    await requestCollectorSnapshot(tabId);
    reply(command, { tab_id: String(tabId), resync_requested: true });
  } else {
    throw new Error(`unsupported host command: ${command.kind}`);
  }
}

function collectorActionError(result, fallback) {
  const stage = String(result?.failure_stage || 'dispatch').replaceAll('|', '_');
  const code = String(result?.failure_code || 'software_action_rejected').replaceAll('|', '_');
  const retrySafe = result?.retry_safe === true ? 'true' : 'false';
  const detail = String(result?.error || fallback).replaceAll('|', '/').slice(0, 320);
  const error = new Error(`saccade_action_error|${stage}|${code}|${retrySafe}|${detail}`);
  error.saccadeStage = stage;
  error.saccadeCode = code;
  error.saccadeRetrySafe = result?.retry_safe === true;
  return error;
}

async function requestCollectorSnapshot(tabId) {
  const result = await collectorMessage(tabId, { kind: 'collector.snapshot' });
  if (!result?.ok) throw new Error(result?.error || 'collector snapshot failed');
}

function acceptCollectorObservation(message, sender) {
  const tabId = sender.tab?.id;
  const session = tabId === undefined ? null : sessions.get(tabId);
  if (!session || !isAuthorized(tabId) || message.payload?.browser_instance_id !== browserInstanceId || message.payload.tab_id !== String(tabId)) return false;
  session.observationReady = true;
  session.documentId = message.payload.document_id;
  session.revision = message.payload.revision;
  delete session.error;
  if (brokerConnectionId) queueEvent({
    kind: message.kind === 'collector.observation_delta' ? 'observation.delta' : 'observation',
    payload: message.payload,
  });
  return true;
}

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== 'saccade.collector' || port.sender.frameId !== 0) return;
  const tabId = port.sender.tab?.id;
  if (tabId === undefined) { port.disconnect(); return; }
  port.onMessage.addListener((message) => {
    if (!['collector.observation', 'collector.observation_delta'].includes(message?.kind)
      || !acceptCollectorObservation(message, port.sender)) port.disconnect();
  });
  port.onDisconnect.addListener(() => {
    const session = sessions.get(tabId);
    if (session?.collectorPort === port) delete session.collectorPort;
  });
  const session = sessions.get(tabId);
  if (session) session.collectorPort = port;
});

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message.kind === 'ui.tab.status' || message.kind === 'ui.tab.share' || message.kind === 'ui.tab.revoke') {
    const run = async () => {
      if (sender.url !== chrome.runtime.getURL('popup.html')) throw new Error('tab access changes require the Saccade popup');
      if (!brokerRuntimePresent()) {
        try { await ensureBrokerConnection(); } catch (error) { scheduleReconnect(error); }
      }
      const tabId = numericTabId(message.tab_id);
      if (message.kind === 'ui.tab.share') {
        const tab = await chrome.tabs.get(tabId);
        if (!isSupportedUrl(tab.url)) throw new Error('Only HTTP and HTTPS tabs can be shared');
        userSharedTabs.add(tabId);
        await persistAcl();
        try {
          await authorizeTab(tabId, { recoverStale: true });
        } catch (error) {
          await revokeTabAccess(tabId);
          throw error;
        }
      } else if (message.kind === 'ui.tab.revoke') {
        await revokeTabAccess(tabId);
      }
      return tabStatus(tabId);
    };
    run().then((status) => respond({ ok: true, status })).catch((error) => {
      respond({ ok: false, error: String(error.message || error).slice(0, 512) });
    });
    return true;
  }
  return false;
});

chrome.tabs.onCreated.addListener((tab) => {
  noteClaimCandidate(tab);
});
chrome.tabs.onRemoved.addListener((tabId) => { sessions.delete(tabId); forgetTab(tabId); persistAcl(); });
chrome.windows.onRemoved.addListener(() => { reconnectAfterWindowRemoval(); });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === RECONNECT_ALARM) connectBroker().catch(scheduleReconnect);
});
chrome.tabs.onUpdated.addListener((tabId, change, tab) => {
  considerClaimCandidate(tabId, change.url || tab?.pendingUrl || tab?.url);
  if (!isAuthorized(tabId)) return;
  if (change.status === 'loading') sessions.delete(tabId);
  // history.pushState/replaceState never reach the Collector's isolated world,
  // so same-document URL changes are relayed here instead.
  if (change.url && change.status === undefined) {
    collectorMessage(tabId, { kind: 'collector.recollect' })
      .catch(() => { /* collector not present in this tab */ });
  }
  if ((change.url || change.status === 'loading' || change.status === 'complete') && isSupportedUrl(tab.url)) {
    authorizeTab(tabId).catch(reportAuthorizationFailure);
  }
});
chrome.runtime.onStartup.addListener(() => {
  connectBroker().catch(scheduleReconnect);
});
chrome.runtime.onInstalled.addListener(() => { connectBroker().catch(scheduleReconnect); });
armReconnectAlarm();
connectBroker().catch(scheduleReconnect);
