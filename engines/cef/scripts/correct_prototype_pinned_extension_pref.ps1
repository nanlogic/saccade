[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$ProfileDir)

$ErrorActionPreference = 'Stop'
$extensionId = 'kfmcgnphhefgadoabheodbhdndhfmonl'
$path = Join-Path $ProfileDir 'Default\Preferences'
$preferences = [IO.File]::ReadAllText($path) | ConvertFrom-Json
if (-not $preferences.extensions) {
  $preferences | Add-Member -NotePropertyName extensions `
    -NotePropertyValue ([pscustomobject]@{})
}
if ($preferences.extensions.PSObject.Properties.Name -contains 'pinned_extensions') {
  $preferences.extensions.pinned_extensions = @($extensionId)
} else {
  $preferences.extensions | Add-Member -NotePropertyName pinned_extensions `
    -NotePropertyValue @($extensionId)
}
[IO.File]::WriteAllText($path,
  ($preferences | ConvertTo-Json -Depth 100 -Compress),
  [Text.UTF8Encoding]::new($false))
$extensionId
