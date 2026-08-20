importScripts('candidate_identity.js', 'protocol.js', 'consent.js');

const { envelope, parseHostMessage, randomToken } = globalThis.SaccadeProtocol;
const { isSupportedUrl, normalizeOrigin } = globalThis.SaccadeConsent;
const LOADED_CANDIDATE = globalThis.SaccadeCandidate;
const NATIVE_HOST = chrome.runtime.getManifest().name.includes('(Development)')
  ? 'com.nanlogic.saccade.dev'
  : 'com.nanlogic.saccade';
const BROWSER_FAMILY = navigator.userAgent.includes('Edg/') ? 'edge' : 'chrome';
const INSTANCE_KEY = 'saccade.browser_instance_id';
const TAB_ACL_KEY = 'saccade.tab_acl';
const BROWSER_SESSION_KEY = 'saccade.browser_session_initialized';
const CLAIM_TTL_MS = 30_000;
const agentOwnedTabs = new Set();
const userSharedTabs = new Set();
const claimedAgentTabs = new Set();
const sessions = new Map();
const authorizationPromises = new Map();
// Session-only, single-use claim intent. Never persisted: a replaced Service
// Worker or an ended Native Host session must fail closed and force a re-arm.
let pendingClaim;
let browserInstanceId;
let nativePort;
let connectPromise;
let reconnectAttempts = 0;
let reconnectTimer;

function sameCandidate(candidate) {
  return candidate?.schema === LOADED_CANDIDATE.schema
    && candidate?.id === LOADED_CANDIDATE.id
    && candidate?.version === LOADED_CANDIDATE.version;
}

async function reloadIfCandidateChanged() {
  try {
    const url = `${chrome.runtime.getURL('candidate.json')}?candidate_check=${Date.now()}`;
    const response = await fetch(url, { cache: 'no-store' });
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
    chrome.storage.session.get(BROWSER_SESSION_KEY),
  ]);
  browserInstanceId = identity[INSTANCE_KEY] || randomToken('browser');
  if (!identity[INSTANCE_KEY]) await chrome.storage.local.set({ [INSTANCE_KEY]: browserInstanceId });
  const freshBrowserSession = browserSession[BROWSER_SESSION_KEY] !== true;
  if (freshBrowserSession) {
    await Promise.all([
      chrome.storage.local.remove(TAB_ACL_KEY),
      chrome.storage.session.set({ [BROWSER_SESSION_KEY]: true }),
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

async function tabStatus(tabId) {
  const tab = await chrome.tabs.get(tabId);
  const supported = isSupportedUrl(tab.url);
  const session = sessions.get(tabId);
  return {
    tab_id: String(tabId), supported, agent_owned: agentOwnedTabs.has(tabId),
    shared: userSharedTabs.has(tabId), authorized: isAuthorized(tabId),
    provenance: tabProvenance(tabId),
    observation_ready: Boolean(session?.observationReady), collector_error: session?.error,
    host_connected: Boolean(nativePort),
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
  try { await chrome.tabs.sendMessage(tabId, { kind: 'collector.deauthorize' }, { frameId: 0 }); } catch (_error) { /* already gone */ }
}

// A claimed tab is Agent On for one Native Host session only. When that session
// ends the claim's authority ends with it; user_shared and tabs.open ownership
// are untouched because their lifecycle is not tied to the Agent session.
async function revokeClaimedTabs() {
  for (const tabId of [...claimedAgentTabs]) await revokeTabAccess(tabId);
}

function post(kind, payload = {}, requestId) {
  if (!nativePort) throw new Error('native host is disconnected');
  nativePort.postMessage(envelope(kind, payload, requestId));
}

const RECONNECT_ALARM = 'saccade.native-host-reconnect';
const RECONNECT_ALARM_DELAY_MS = 30_000;

function scheduleReconnect(error) {
  if (error) console.error(`Saccade reconnect scheduled: ${String(error.message || error)}`);
  if (reconnectTimer) {
    chrome.alarms.create(RECONNECT_ALARM, { when: Date.now() + RECONNECT_ALARM_DELAY_MS });
    return;
  }
  const delay = Math.min(250 * (2 ** reconnectAttempts++), 4000);
  chrome.alarms.create(RECONNECT_ALARM, { when: Date.now() + RECONNECT_ALARM_DELAY_MS });
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connectHost().catch(scheduleReconnect); }, delay);
}

async function reconnectAfterWindowRemoval() {
  chrome.alarms.create(RECONNECT_ALARM, { when: Date.now() + RECONNECT_ALARM_DELAY_MS });
  // A healthy Native Messaging port keeps the MV3 worker reachable while the
  // browser has no normal windows. Disconnecting it here would remove the only
  // route through which tabs.open can create the next window.
  if (nativePort) return;
  try { await connectHost(); } catch (_error) { scheduleReconnect(); }
}

async function settleReconnect(port) {
  if (nativePort !== port) return;
  reconnectAttempts = 0;
  const windows = await chrome.windows.getAll({ windowTypes: ['normal'] });
  if (windows.length) chrome.alarms.clear(RECONNECT_ALARM);
  else chrome.alarms.create(RECONNECT_ALARM, { when: Date.now() + RECONNECT_ALARM_DELAY_MS });
}

async function connectHost() {
  if (nativePort) return;
  if (connectPromise) return connectPromise;
  connectPromise = (async () => {
    await reloadIfCandidateChanged();
    if (!browserInstanceId) await initialize();
    const port = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort = port;
    port.onDisconnect.addListener(() => {
      const detail = chrome.runtime.lastError?.message;
      if (detail) console.error(`Saccade Native Host disconnected: ${detail}`);
      if (nativePort === port) nativePort = undefined;
      pendingClaim = undefined;
      revokeClaimedTabs().catch(reportAuthorizationFailure);
      scheduleReconnect();
    });
    port.onMessage.addListener((message) => {
      const command = parseHostMessage(message);
      if (!command) return;
      handleHostCommand(command).catch((error) => {
        if (command.requestId !== undefined && nativePort) post('response', { error: String(error.message || error) }, command.requestId);
      });
    });
    post('hello', {
      browser_instance_id: browserInstanceId,
      extension_candidate: LOADED_CANDIDATE,
      browser_family: BROWSER_FAMILY,
      development: NATIVE_HOST.endsWith('.dev'),
      wake_url: chrome.runtime.getURL('popup.html'),
    });
    setTimeout(() => { settleReconnect(port).catch(scheduleReconnect); }, 5000);
    for (const tabId of new Set([...agentOwnedTabs, ...userSharedTabs])) {
      const hadCurrentObservation = Boolean(sessions.get(tabId)?.observationReady);
      try {
        await authorizeTab(tabId);
        if (hadCurrentObservation) await requestCollectorSnapshot(tabId);
      } catch (error) { reportAuthorizationFailure(error); }
    }
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
      const ping = await chrome.tabs.sendMessage(tabId, { kind: 'collector.ping' }, { frameId: 0 });
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
    const configured = await chrome.tabs.sendMessage(tabId, { kind: 'collector.configure', config: {
      browserInstanceId, tabId: String(tabId), frameId: `frame.${tabId}.0`,
    } }, { frameId: 0 });
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
  if (command.requestId !== undefined) post('response', payload, command.requestId);
}

async function handleHostCommand(command) {
  const payload = command.payload;
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
    reply(command, claimed);
    authorizeTab(numericTabId(claimed.tab_id)).catch(reportAuthorizationFailure);
  } else if (command.kind === 'tabs.open') {
    if (payload.claim !== undefined) throw new Error('claim must be arm or confirm');
    const tab = await openAgentTab(navigableUrl(payload.url), payload.active !== false);
    if (tab.id === undefined) throw new Error('browser did not return a tab identity');
    agentOwnedTabs.add(tab.id);
    await persistAcl();
    reply(command, { tab_id: String(tab.id), opened: true });
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
    const tab = await chrome.tabs.get(tabId);
    const browserWindow = await chrome.windows.get(tab.windowId);
    let focusChanged = false;
    if (!browserWindow.focused) { await chrome.windows.update(tab.windowId, { focused: true }); focusChanged = true; }
    if (!tab.active) { await chrome.tabs.update(tabId, { active: true }); focusChanged = true; }
    if (focusChanged) await new Promise((resolve) => setTimeout(resolve, 100));
    const result = await chrome.tabs.sendMessage(tabId, { kind: 'collector.prepare_action', request: payload }, { frameId: 0 });
    if (!result?.ok) throw collectorActionError(result, 'action preparation failed');
    reply(command, result.prepared);
  } else if (command.kind === 'soft_click') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    const result = await chrome.tabs.sendMessage(tabId, { kind: 'collector.soft_click', request: payload }, { frameId: 0 });
    if (!result?.ok) throw collectorActionError(result, 'soft click failed');
    reply(command, result.result);
  } else if (command.kind === 'soft_action') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    const result = await chrome.tabs.sendMessage(tabId, { kind: 'collector.soft_action', request: payload }, { frameId: 0 });
    if (!result?.ok) throw collectorActionError(result, 'software action failed');
    reply(command, result.result);
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
  return new Error(`saccade_action_error|${stage}|${code}|${retrySafe}|${detail}`);
}

async function requestCollectorSnapshot(tabId) {
  const result = await chrome.tabs.sendMessage(
    tabId, { kind: 'collector.snapshot' }, { frameId: 0 },
  );
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
  if (nativePort) post(message.kind === 'collector.observation_delta' ? 'observation.delta' : 'observation', message.payload);
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
  if (alarm.name === RECONNECT_ALARM) connectHost().catch(scheduleReconnect);
});
chrome.tabs.onUpdated.addListener((tabId, change, tab) => {
  considerClaimCandidate(tabId, change.url || tab?.pendingUrl || tab?.url);
  if (!isAuthorized(tabId)) return;
  if (change.status === 'loading') sessions.delete(tabId);
  // history.pushState/replaceState never reach the Collector's isolated world,
  // so same-document URL changes are relayed here instead.
  if (change.url && change.status === undefined) {
    chrome.tabs.sendMessage(tabId, { kind: 'collector.recollect' }, { frameId: 0 })
      .catch(() => { /* collector not present in this tab */ });
  }
  if ((change.url || change.status === 'loading' || change.status === 'complete') && isSupportedUrl(tab.url)) {
    authorizeTab(tabId).catch(reportAuthorizationFailure);
  }
});
chrome.runtime.onStartup.addListener(() => {
  connectHost().catch(scheduleReconnect);
});
chrome.runtime.onInstalled.addListener(() => { connectHost().catch(scheduleReconnect); });
connectHost().catch(scheduleReconnect);
