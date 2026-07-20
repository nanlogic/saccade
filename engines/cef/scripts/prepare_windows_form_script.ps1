[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Destination
)

$ErrorActionPreference = 'Stop'
$text = [System.IO.File]::ReadAllText((Resolve-Path $Source))
$text = $text.Replace("`r`n", "`n")
$prefix = 'R"SACCADE_FORM_JS('
$suffix = ')SACCADE_FORM_JS";'
$start = $text.IndexOf($prefix, [StringComparison]::Ordinal)
$end = $text.LastIndexOf($suffix, [StringComparison]::Ordinal)
if ($start -lt 0 -or $end -le $start) {
  throw 'Build 64 form script raw-literal boundary was not found'
}
$bodyStart = $start + $prefix.Length
$body = $text.Substring($bodyStart, $end - $bodyStart)
$chunks = [System.Collections.Generic.List[string]]::new()
for ($offset = 0; $offset -lt $body.Length; $offset += 16000) {
  $length = [Math]::Min(16000, $body.Length - $offset)
  $chunks.Add($body.Substring($offset, $length))
}
$separator = ')SACCADE_FORM_JS"' + "`n" + 'R"SACCADE_FORM_JS('
$generated = $text.Substring(0, $bodyStart) +
  [string]::Join($separator, $chunks) + $text.Substring($end)
[System.IO.File]::WriteAllText($Destination, $generated,
  [System.Text.UTF8Encoding]::new($false))
