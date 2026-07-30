importScripts('protocol.js', 'consent.js');

const { envelope, parseHostMessage, randomToken } = globalThis.SaccadeProtocol;
const { isSupportedUrl } = globalThis.SaccadeConsent;
const NATIVE_HOST = 'com.nanlogic.saccade.dev';
const INSTANCE_KEY = 'saccade.browser_instance_id';
const TAB_ACL_KEY = 'saccade.tab_acl';
const agentOwnedTabs = new Set();
const userSharedTabs = new Set();
const sessions = new Map();
const authorizationPromises = new Map();
let browserInstanceId;
let nativePort;
let connectPromise;
let reconnectAttempts = 0;
let reconnectTimer;

async function persistAcl() {
  await chrome.storage.session.set({ [TAB_ACL_KEY]: { agent: [...agentOwnedTabs], shared: [...userSharedTabs] } });
}

async function initialize() {
  const [identity, storedAcl] = await Promise.all([chrome.storage.local.get(INSTANCE_KEY), chrome.storage.session.get(TAB_ACL_KEY)]);
  browserInstanceId = identity[INSTANCE_KEY] || randomToken('browser');
  if (!identity[INSTANCE_KEY]) await chrome.storage.local.set({ [INSTANCE_KEY]: browserInstanceId });
  const acl = storedAcl[TAB_ACL_KEY] || {};
  for (const value of acl.agent || []) if (Number.isSafeInteger(value)) agentOwnedTabs.add(value);
  for (const value of acl.shared || []) if (Number.isSafeInteger(value)) userSharedTabs.add(value);
  for (const tabId of new Set([...agentOwnedTabs, ...userSharedTabs])) {
    try { await chrome.tabs.get(tabId); } catch (_error) { agentOwnedTabs.delete(tabId); userSharedTabs.delete(tabId); }
  }
  await persistAcl();
}

function isAuthorized(tabId) { return agentOwnedTabs.has(tabId) || userSharedTabs.has(tabId); }

async function tabStatus(tabId) {
  const tab = await chrome.tabs.get(tabId);
  const supported = isSupportedUrl(tab.url);
  const session = sessions.get(tabId);
  return {
    tab_id: String(tabId), supported, agent_owned: agentOwnedTabs.has(tabId),
    shared: userSharedTabs.has(tabId), authorized: isAuthorized(tabId),
    observation_ready: Boolean(session?.last), collector_error: session?.error,
    host_connected: Boolean(nativePort),
  };
}

async function revokeSharedTab(tabId) {
  userSharedTabs.delete(tabId);
  sessions.delete(tabId);
  await persistAcl();
  try { await chrome.tabs.sendMessage(tabId, { kind: 'collector.deauthorize' }, { frameId: 0 }); } catch (_error) { /* already gone */ }
}

function post(kind, payload = {}, requestId) {
  if (!nativePort) throw new Error('native host is disconnected');
  nativePort.postMessage(envelope(kind, payload, requestId));
}

function scheduleReconnect() {
  if (reconnectTimer || reconnectAttempts >= 5) return;
  const delay = Math.min(250 * (2 ** reconnectAttempts++), 4000);
  reconnectTimer = setTimeout(() => { reconnectTimer = null; connectHost().catch(scheduleReconnect); }, delay);
}

async function connectHost() {
  if (nativePort) return;
  if (connectPromise) return connectPromise;
  connectPromise = (async () => {
    if (!browserInstanceId) await initialize();
    const port = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort = port;
    port.onDisconnect.addListener(() => {
      const detail = chrome.runtime.lastError?.message;
      if (detail) console.error(`Saccade Native Host disconnected: ${detail}`);
      if (nativePort === port) nativePort = undefined;
      scheduleReconnect();
    });
    port.onMessage.addListener((message) => {
      const command = parseHostMessage(message);
      if (!command) return;
      handleHostCommand(command).catch((error) => {
        if (command.requestId !== undefined && nativePort) post('response', { error: String(error.message || error) }, command.requestId);
      });
    });
    post('hello', { browser_instance_id: browserInstanceId });
    setTimeout(() => { if (nativePort === port) reconnectAttempts = 0; }, 5000);
    for (const tabId of new Set([...agentOwnedTabs, ...userSharedTabs])) authorizeTab(tabId).catch(reportAuthorizationFailure);
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

async function authorizeTab(tabId) {
  const tab = await chrome.tabs.get(tabId);
  if (!isSupportedUrl(tab.url)) throw new Error('tab URL is not supported');
  const existing = authorizationPromises.get(tabId);
  if (existing?.url === tab.url) return existing.promise;
  if (existing) {
    return existing.promise.catch(() => {}).then(() => authorizeTab(tabId));
  }
  const entry = { url: tab.url, promise: null };
  entry.promise = authorizeTabInner(tabId, tab.url).finally(() => {
    if (authorizationPromises.get(tabId) === entry) authorizationPromises.delete(tabId);
  });
  authorizationPromises.set(tabId, entry);
  return entry.promise;
}

async function authorizeTabInner(tabId, expectedUrl) {
  if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
  const tab = await chrome.tabs.get(tabId);
  if (tab.url !== expectedUrl) throw new Error('tab URL changed during collector authorization');
  if (!isSupportedUrl(tab.url)) throw new Error('tab URL is not supported');
  const prior = sessions.get(tabId);
  if (prior?.url === tab.url && (prior.configuring || prior.configured)) return;
  sessions.set(tabId, { last: null, url: tab.url, configuring: true });
  let ready = false;
  try { ready = (await chrome.tabs.sendMessage(tabId, { kind: 'collector.ping' }, { frameId: 0 }))?.ok === true; } catch (_error) { /* inject below */ }
  if (!ready) {
    await chrome.scripting.executeScript({ target: { tabId, frameIds: [0] }, files: [
      'src/protocol.js', 'src/consent.js', 'src/controls/common.js', 'src/controls/button.js', 'src/controls/link.js',
      'src/controls/text_field.js', 'src/controls/search_field.js', 'src/controls/text_area.js',
      'src/controls/content_editable.js', 'src/controls/spin_button.js',
      'src/controls/checkbox.js', 'src/controls/radio.js', 'src/controls/switch.js', 'src/controls/select.js',
      'src/controls/tab.js', 'src/controls/menu_item.js', 'src/controls/reflex_target.js', 'src/controls/file_input.js',
      'src/controls/registry.js', 'src/collector.js',
    ] });
  }
  try {
    const configured = await chrome.tabs.sendMessage(tabId, { kind: 'collector.configure', config: {
      browserInstanceId, tabId: String(tabId), frameId: `frame.${tabId}.0`,
    } }, { frameId: 0 });
    if (!configured?.ok) throw new Error(configured?.error || 'collector configuration failed');
    const session = sessions.get(tabId);
    if (session?.url === tab.url) { session.configuring = false; session.configured = true; }
  } catch (error) {
    const detail = String(error?.message || error).replace(/[\r\n]+/g, ' ').slice(0, 512);
    sessions.set(tabId, { last: null, url: tab.url, error: detail });
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
        const item = { tab_id: String(tabId), title: tab.title || '', url: tab.url || '', active: Boolean(tab.active), observation_ready: Boolean(session?.last) };
        if (session?.error) item.collector_error = session.error;
        tabs.push(item);
      } catch (_error) { agentOwnedTabs.delete(tabId); userSharedTabs.delete(tabId); }
    }
    await persistAcl();
    reply(command, { tabs });
  } else if (command.kind === 'tabs.open') {
    const tab = await chrome.tabs.create({ url: navigableUrl(payload.url), active: payload.active !== false });
    if (tab.id === undefined) throw new Error('browser did not return a tab identity');
    agentOwnedTabs.add(tab.id);
    await persistAcl();
    reply(command, { tab_id: String(tab.id), opened: true });
    const current = await chrome.tabs.get(tab.id);
    if (isSupportedUrl(current.url)) authorizeTab(tab.id).catch(reportAuthorizationFailure);
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
    if (!result?.ok) throw new Error(result?.error || 'action preparation failed');
    reply(command, result.prepared);
  } else if (command.kind === 'soft_click') {
    const tabId = numericTabId(payload.tab_id);
    if (!isAuthorized(tabId)) throw new Error('tab is not authorized');
    const result = await chrome.tabs.sendMessage(tabId, { kind: 'collector.soft_click', request: payload }, { frameId: 0 });
    if (!result?.ok) throw new Error(result?.error || 'soft click failed');
    reply(command, result.result);
  } else {
    throw new Error(`unsupported host command: ${command.kind}`);
  }
}

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
          await authorizeTab(tabId);
        } catch (error) {
          await revokeSharedTab(tabId);
          throw error;
        }
      } else if (message.kind === 'ui.tab.revoke') {
        if (agentOwnedTabs.has(tabId)) throw new Error('Agent-owned tabs are revoked by closing the tab');
        await revokeSharedTab(tabId);
      }
      return tabStatus(tabId);
    };
    run().then((status) => respond({ ok: true, status })).catch((error) => {
      respond({ ok: false, error: String(error.message || error).slice(0, 512) });
    });
    return true;
  }
  if (message.kind !== 'collector.observation') return false;
  const tabId = sender.tab?.id;
  const session = tabId === undefined ? null : sessions.get(tabId);
  if (!session || !isAuthorized(tabId) || message.payload?.browser_instance_id !== browserInstanceId || message.payload.tab_id !== String(tabId)) {
    respond({ ok: false }); return false;
  }
  session.last = message.payload;
  delete session.error;
  if (nativePort) post('observation', message.payload);
  respond({ ok: true });
  return false;
});

chrome.tabs.onCreated.addListener((tab) => {
  if (tab.id === undefined || tab.openerTabId === undefined || !agentOwnedTabs.has(tab.openerTabId)) return;
  agentOwnedTabs.add(tab.id); persistAcl();
});
chrome.tabs.onRemoved.addListener((tabId) => { sessions.delete(tabId); agentOwnedTabs.delete(tabId); userSharedTabs.delete(tabId); persistAcl(); });
chrome.tabs.onUpdated.addListener((tabId, change, tab) => {
  if (!isAuthorized(tabId)) return;
  if (change.status === 'loading') sessions.delete(tabId);
  if ((change.url || change.status === 'loading' || change.status === 'complete') && isSupportedUrl(tab.url)) {
    authorizeTab(tabId).catch(reportAuthorizationFailure);
  }
});
chrome.runtime.onStartup.addListener(() => { connectHost().catch(scheduleReconnect); });
chrome.runtime.onInstalled.addListener(() => { connectHost().catch(scheduleReconnect); });
connectHost().catch(scheduleReconnect);
