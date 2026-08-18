'use strict';

const state = document.getElementById('state');
const phase = document.getElementById('phase');
const detail = document.getElementById('detail');
const toggle = document.getElementById('toggle');
const toggleLabel = document.getElementById('toggle-label');
const host = document.getElementById('host');
const hostDot = document.getElementById('host-dot');
const statusDot = document.getElementById('status-dot');
const devBadge = document.getElementById('dev-badge');
let tabId;
let current;

function setStatus(primary, secondary, tone) {
  state.textContent = primary;
  phase.textContent = secondary;
  statusDot.className = `status-dot ${tone}`;
}

function render(status) {
  current = status;
  host.textContent = `Runtime ${status.host_connected ? 'connected' : 'disconnected'}`;
  hostDot.classList.toggle('connected', status.host_connected);
  toggle.classList.remove('danger', 'locked');
  if (!status.supported) {
    setStatus('Agent Off', 'Unsupported', 'off');
    detail.textContent = 'Saccade can share HTTP and HTTPS pages only.';
    toggleLabel.textContent = 'Unsupported tab';
    toggle.classList.add('locked');
    toggle.disabled = true;
  } else if (status.authorized) {
    setStatus('Agent On', status.observation_ready ? 'Ready' : 'Starting', status.observation_ready ? 'on' : 'pending');
    const source = status.provenance === 'agent_client'
      ? 'An agent claimed this tab for this browser session. Stop sharing at any time without closing it.'
      : 'Saccade access is on for this tab. Stop sharing at any time without closing it.';
    detail.textContent = status.collector_error || source;
    toggleLabel.textContent = 'Stop sharing';
    toggle.classList.add('danger');
    toggle.disabled = false;
  } else {
    setStatus('Agent Off', 'Private', 'off');
    detail.textContent = 'Only this tab will be shared. Password, SSN, and EIN values stay protected.';
    toggleLabel.textContent = 'Share this tab';
    toggle.disabled = false;
  }
}

async function command(kind) {
  toggle.disabled = true;
  const response = await chrome.runtime.sendMessage({ kind, tab_id: String(tabId) });
  if (!response?.ok) throw new Error(response?.error || 'Saccade did not respond');
  render(response.status);
}

toggle.addEventListener('click', () => {
  command(current?.authorized ? 'ui.tab.revoke' : 'ui.tab.share').catch((error) => {
    state.textContent = 'Could not update access';
    phase.textContent = 'Error';
    statusDot.className = 'status-dot error';
    detail.textContent = String(error.message || error);
    toggle.disabled = false;
  });
});

(async () => {
  devBadge.hidden = !chrome.runtime.getManifest().name.includes('(Development)');
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (tab?.id === undefined) throw new Error('No active tab');
  tabId = tab.id;
  await command('ui.tab.status');
})().catch((error) => {
  setStatus('Unavailable', 'Error', 'error');
  detail.textContent = String(error.message || error);
  toggleLabel.textContent = 'Unavailable';
  toggle.classList.add('locked');
  toggle.disabled = true;
});
