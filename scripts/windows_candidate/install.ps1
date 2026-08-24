param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-p]{32}$')]
  [string]$ExtensionId
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$TemplatePath = Join-Path $Root 'release-template.json'
$ReleasePath = Join-Path $Root 'candidate-release.json'
$RuntimePath = (Resolve-Path (Join-Path $Root 'runtime\saccade-runtime.exe')).Path
$Release = Get-Content -Raw $TemplatePath | ConvertFrom-Json
$Release.native_host.allowed_origins = @("chrome-extension://$ExtensionId/")
$Release.artifacts.'win32-x64'.url = ([System.Uri]$RuntimePath).AbsoluteUri
$ReleaseJson = $Release | ConvertTo-Json -Depth 12
# Windows PowerShell 5.1 writes a BOM for -Encoding UTF8. The setup CLI reads
# this as JSON, so write interoperable UTF-8 without a BOM on every PowerShell.
[System.IO.File]::WriteAllText(
  $ReleasePath,
  $ReleaseJson,
  [System.Text.UTF8Encoding]::new($false)
)

Write-Host 'Installing the unsigned Saccade Windows test candidate...'
& node (Join-Path $Root 'package\bin\saccade-setup.js') --release-manifest $ReleasePath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ''
Write-Host 'Installation finished. Keep the unpacked Extension loaded, then restart Codex or Claude.'
Write-Host 'Saccade opens known test URLs as Agent-owned tabs automatically; no tab sharing is required for the normal route.'
Write-Host "Run doctor with: node `"$Root\package\bin\saccade-setup.js`" doctor"
