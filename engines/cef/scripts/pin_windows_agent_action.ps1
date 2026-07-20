[CmdletBinding()]
param(
  [string]$UserDataDir = '',
  [string]$ExtensionId = 'kfmcgnphhefgadoabheodbhdndhfmonl'
)

$ErrorActionPreference = 'Stop'
if (-not $UserDataDir) {
  $UserDataDir = Join-Path $env:LOCALAPPDATA 'Saccade\CEF\Profiles\default'
}
$profileDir = Join-Path $UserDataDir 'Default'
New-Item -ItemType Directory -Path $profileDir -Force | Out-Null
$preferencesPath = Join-Path $profileDir 'Preferences'

if (Test-Path -LiteralPath $preferencesPath) {
  $raw = [IO.File]::ReadAllText($preferencesPath)
  $preferences = if ([string]::IsNullOrWhiteSpace($raw)) {
    [pscustomobject]@{}
  } else {
    $raw | ConvertFrom-Json
  }
} else {
  $preferences = [pscustomobject]@{}
}
if (-not $preferences.PSObject.Properties['extensions']) {
  $preferences | Add-Member -NotePropertyName extensions -NotePropertyValue ([pscustomobject]@{})
}
$extensions = $preferences.extensions
$pinned = @()
if ($extensions.PSObject.Properties['pinned_extensions']) {
  $pinned = @($extensions.pinned_extensions)
}
$pinned = @($ExtensionId) + @($pinned | Where-Object { $_ -ne $ExtensionId })
if ($extensions.PSObject.Properties['pinned_extensions']) {
  $extensions.pinned_extensions = $pinned
} else {
  $extensions | Add-Member -NotePropertyName pinned_extensions -NotePropertyValue $pinned
}

$temporary = "$preferencesPath.saccade.tmp"
$json = $preferences | ConvertTo-Json -Depth 100 -Compress
[IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $temporary -Destination $preferencesPath -Force
$preferencesPath
