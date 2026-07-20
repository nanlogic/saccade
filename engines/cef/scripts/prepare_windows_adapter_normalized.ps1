[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Destination
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) `
  ("saccade-adapter-{0}.cc" -f [Guid]::NewGuid().ToString('N'))
try {
  $text = [System.IO.File]::ReadAllText((Resolve-Path $Source))
  $text = $text.Replace("`r`n", "`n")
  [System.IO.File]::WriteAllText($temporary, $text,
    [System.Text.UTF8Encoding]::new($false))
  & (Join-Path $scriptRoot 'prepare_windows_adapter.ps1') `
    -Source $temporary -Destination $Destination
} finally {
  Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
}
