[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$PackageDir)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\..')).Path
$extensionDir = Join-Path $PackageDir 'extensions\saccade-new-tab'
$manifestPath = Join-Path $extensionDir 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath)) {
  throw "Missing packaged extension: $manifestPath"
}

$manifest = [ordered]@{
  manifest_version = 3
  name = 'Saccade New Tab'
  version = '0.1.0'
  description = 'Uses the Saccade identity for the browser new-tab page and exposes the tab-scoped Agent action.'
  chrome_url_overrides = [ordered]@{ newtab = 'newtab.html' }
  background = [ordered]@{ service_worker = 'agent-action.js' }
  action = [ordered]@{
    default_title = 'Agent Off - current tab'
    default_icon = [ordered]@{
      '16' = 'Saccade.png'
      '24' = 'Saccade.png'
      '32' = 'Saccade.png'
    }
  }
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8
Copy-Item -LiteralPath (Join-Path $repoRoot 'engines\cef\assets\saccade-icon-windows.png') `
  -Destination (Join-Path $extensionDir 'Saccade.png') -Force

@'
const enabledTabs = new Set();

async function render(tabId, enabled) {
  await chrome.action.setTitle({
    tabId,
    title: enabled ? 'Agent On - current tab' : 'Agent Off - current tab',
  });
  await chrome.action.setBadgeText({tabId, text: enabled ? 'ON' : ''});
  await chrome.action.setBadgeBackgroundColor({tabId, color: '#1A73E8'});
}

chrome.action.onClicked.addListener(async (tab) => {
  if (!tab.id) return;
  const enabled = !enabledTabs.has(tab.id);
  if (enabled) enabledTabs.add(tab.id);
  else enabledTabs.delete(tab.id);
  await render(tab.id, enabled);
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status !== 'loading') return;
  enabledTabs.delete(tabId);
  void render(tabId, false);
});

chrome.tabs.onRemoved.addListener((tabId) => enabledTabs.delete(tabId));
'@ | Set-Content -LiteralPath (Join-Path $extensionDir 'agent-action.js') -Encoding utf8

$manifestPath
