[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PackageDir,
  [Parameter(Mandatory = $true)][string]$ProfileDir
)

$ErrorActionPreference = 'Stop'
$extensionId = 'kfmcgnphhefgadoabheodbhdndhfmonl'
$publicKey = 'MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA2T9V7R1OKdctWcR+ZWJ7ZWLuYcMcOfbGZj61uSxXIPtw/xv5QxLBGqslqqlPdwKcoiXTWCWhK1OIIJ4EbF4SFL4LGALjKM1c9gv02TIltevDqDksL3VUj5fghVP8QTqjp4kMwxUPgZILGEfwt75GoMHEsSd2ccmtApu5eyvtU9ZfrIA1HfgmTaUoDXR4zJ9tIIyncSapoWzh4yIC9ksgWlO8qOGCgVSFzp8C9Sdw3gIs9rBs21aVLxcHnvYvZwE90wHSRKtNvXqVBtxJee2rlzCMJqQVqZrSiH0HDhouGupKGLvWBYPk9wsqvevwpufaB9dEUK1WGRNT5PbmWvsW6wIDAQAB'

$manifestPath = Join-Path $PackageDir 'extensions\saccade-new-tab\manifest.json'
$manifest = [IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
if ($manifest.PSObject.Properties.Name -contains 'key') {
  $manifest.key = $publicKey
} else {
  $manifest | Add-Member -NotePropertyName key -NotePropertyValue $publicKey
}
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8),
  [Text.UTF8Encoding]::new($false))

$preferencesPath = Join-Path $ProfileDir 'Default\Preferences'
$preferences = [IO.File]::ReadAllText($preferencesPath) | ConvertFrom-Json
if (-not $preferences.toolbar) {
  $preferences | Add-Member -NotePropertyName toolbar -NotePropertyValue ([pscustomobject]@{})
}
if ($preferences.toolbar.PSObject.Properties.Name -contains 'pinned_actions') {
  $preferences.toolbar.pinned_actions = @($extensionId)
} else {
  $preferences.toolbar | Add-Member -NotePropertyName pinned_actions `
    -NotePropertyValue @($extensionId)
}
[IO.File]::WriteAllText($preferencesPath,
  ($preferences | ConvertTo-Json -Depth 100 -Compress),
  [Text.UTF8Encoding]::new($false))

$extensionId
