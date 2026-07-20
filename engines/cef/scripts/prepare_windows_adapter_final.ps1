[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Destination
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $scriptRoot 'prepare_windows_adapter_normalized.ps1') `
  -Source $Source -Destination $Destination
$text = [System.IO.File]::ReadAllText($Destination)
$old = '#define O_NOFOLLOW kSaccadeOpenNoFollow'
$new = $old + "`n#define O_CLOEXEC 0"
if (-not $text.Contains($old)) {
  throw 'Windows adapter transform did not expose the open-flag compatibility block'
}
$text = $text.Replace($old, $new)
[System.IO.File]::WriteAllText($Destination, $text,
  [System.Text.UTF8Encoding]::new($false))
