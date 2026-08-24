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
$Release | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 $ReleasePath

Write-Host 'Installing the unsigned Saccade Windows test candidate...'
& node (Join-Path $Root 'package\bin\saccade-setup.js') --release-manifest $ReleasePath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ''
Write-Host 'Installation finished. Keep the unpacked Extension loaded, share one tab, then restart Codex or Claude.'
Write-Host "Run doctor with: node `"$Root\package\bin\saccade-setup.js`" doctor"
