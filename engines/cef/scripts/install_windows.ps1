[CmdletBinding()]
param(
  [string]$SourceDir = '',
  [string]$InstallDir = '',
  [string]$ProfileRoot = '',
  [switch]$NoLaunch,
  [switch]$SkipSystemIntegration,
  [switch]$TestFailAfterSwap
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
& (Join-Path $scriptRoot 'install_windows_staged.ps1') @PSBoundParameters
