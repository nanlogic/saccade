[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved)
$old = 'set(CEF_TARGET "cefsimple")'
$new = 'set(CEF_TARGET "Saccade")'
if ($text.Contains($old)) {
  $text = $text.Replace($old, $new)
} elseif (-not $text.Contains($new)) {
  throw 'CEF target name declaration was not recognized'
}
[IO.File]::WriteAllText($resolved, $text, [Text.UTF8Encoding]::new($false))
