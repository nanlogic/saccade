[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
& icacls.exe $resolved /grant '*S-1-15-2-2:(OI)(CI)(RX)' /T /C | Out-Null
if ($LASTEXITCODE -ne 0) {
  throw "Failed to grant CEF sandbox LPAC read/execute access: $resolved"
}
