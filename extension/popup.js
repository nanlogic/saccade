'use strict';

const state = document.getElementById('state');
const detail = document.getElementById('detail');
const toggle = document.getElementById('toggle');
const host = document.getElementById('host');
let tabId;
let current;

function render(status) {
  current = status;
  host.textContent = `Runtime: ${status.host_connected ? 'connected' : 'disconnected'}`;
  toggle.classList.remove('danger');
  if (!status.supported) {
    state.textContent = 'Agent Off';
    detail.textContent = 'Saccade can share HTTP and HTTPS pages only.';
    toggle.textContent = 'Unsupported tab';
    toggle.disabled = true;
  } else if (status.agent_owned) {
    state.textContent = status.observation_ready ? 'Agent On · ready' : 'Agent On · starting';
    detail.textContent = status.collector_error || 'This tab was opened by the Agent. Close it to revoke access.';
    toggle.textContent = 'Agent-owned tab';
    toggle.disabled = true;
  } else if (status.shared) {
    state.textContent = status.observation_ready ? 'Agent On · ready' : 'Agent On · starting';
    detail.textContent = status.collector_error || 'The Agent can observe and use controls in this tab until you revoke access or end the browser session.';
    toggle.textContent = 'Stop sharing';
    toggle.classList.add('danger');
    toggle.disabled = false;
  } else {
    state.textContent = 'Agent Off';
    detail.textContent = 'Only this tab will be shared. Passwords, OTPs, and editable values are not exposed in observations.';
    toggle.textContent = 'Share this tab';
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
  command(current?.shared ? 'ui.tab.revoke' : 'ui.tab.share').catch((error) => {
    state.textContent = 'Could not update access';
    detail.textContent = String(error.message || error);
    toggle.disabled = false;
  });
});

(async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (tab?.id === undefined) throw new Error('No active tab');
  tabId = tab.id;
  await command('ui.tab.status');
})().catch((error) => {
  state.textContent = 'Unavailable';
  detail.textContent = String(error.message || error);
  toggle.disabled = true;
});
