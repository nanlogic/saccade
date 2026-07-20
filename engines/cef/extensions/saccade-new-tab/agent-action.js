const HOST = "com.nanlogic.saccade_agent";

async function send(command) {
  return chrome.runtime.sendNativeMessage(HOST, { command });
}

async function render(tabId, state, error = "") {
  const on = state === "on";
  const paused = state === "paused";
  const unavailable = state === "unavailable";
  await chrome.action.setBadgeText({
    tabId,
    text: error ? "!" : on ? "ON" : paused ? "||" : "",
  });
  await chrome.action.setBadgeBackgroundColor({
    tabId,
    color: error ? "#D93025" : on ? "#1A73E8" : "#5F6368",
  });
  const label = error
    ? `Agent unavailable - ${error}`
    : unavailable
      ? "Agent unavailable for this tab"
      : paused
        ? "Agent Paused - current tab"
        : `Agent ${on ? "On" : "Off"} - current tab`;
  await chrome.action.setTitle({ tabId, title: label });
}

async function refresh(tabId) {
  if (typeof tabId !== "number") return;
  try {
    const response = await send("state");
    if (!response?.ok) throw new Error(response?.error || "native bridge failed");
    await render(tabId, response.state);
  } catch (error) {
    await render(tabId, "unavailable", String(error?.message || error));
  }
}

chrome.action.onClicked.addListener(async (tab) => {
  if (typeof tab.id !== "number") return;
  try {
    const response = await send("toggle");
    if (!response?.ok) throw new Error(response?.error || "native bridge failed");
    await render(tab.id, response.state);
  } catch (error) {
    await render(tab.id, "unavailable", String(error?.message || error));
  }
});

chrome.tabs.onActivated.addListener(({ tabId }) => refresh(tabId));
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === "complete" && tab.active) refresh(tabId);
});
chrome.windows.onFocusChanged.addListener(async (windowId) => {
  if (windowId === chrome.windows.WINDOW_ID_NONE) return;
  const [tab] = await chrome.tabs.query({ active: true, windowId });
  if (tab) refresh(tab.id);
});
chrome.runtime.onStartup.addListener(async () => {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (tab) refresh(tab.id);
});
