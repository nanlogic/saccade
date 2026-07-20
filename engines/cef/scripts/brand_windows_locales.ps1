[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PackageDir
)

$ErrorActionPreference = 'Stop'
$PackageDir = (Resolve-Path -LiteralPath $PackageDir).Path

function Brand-LocalePack([string]$Path) {
  [byte[]]$bytes = [IO.File]::ReadAllBytes($Path)
  if ([BitConverter]::ToUInt32($bytes, 0) -ne 5) {
    throw "Unsupported Chromium DataPack version in $Path"
  }
  $encodingId = $bytes[4]
  $encoding = switch ($encodingId) {
    1 { [Text.Encoding]::UTF8 }
    2 { [Text.Encoding]::Unicode }
    default { return 0 }
  }
  $resourceCount = [BitConverter]::ToUInt16($bytes, 8)
  $aliasCount = [BitConverter]::ToUInt16($bytes, 10)
  $entries = New-Object 'object[]' ($resourceCount + 1)
  for ($index = 0; $index -le $resourceCount; $index++) {
    $entryOffset = 12 + ($index * 6)
    $entries[$index] = [pscustomobject]@{
      Id = [BitConverter]::ToUInt16($bytes, $entryOffset)
      Offset = [BitConverter]::ToUInt32($bytes, $entryOffset + 2)
    }
  }
  $aliasOffset = 12 + (($resourceCount + 1) * 6)
  $dataOffset = $aliasOffset + ($aliasCount * 4)
  if ($entries[0].Offset -ne $dataOffset) {
    throw "Unexpected Chromium DataPack index layout in $Path"
  }

  $payloads = New-Object 'System.Collections.Generic.List[byte[]]'
  $replacementCount = 0
  for ($index = 0; $index -lt $resourceCount; $index++) {
    $start = [int]$entries[$index].Offset
    $end = [int]$entries[$index + 1].Offset
    $length = $end - $start
    $text = $encoding.GetString($bytes, $start, $length)
    $matches = [regex]::Matches($text, 'Chromium').Count
    if ($matches -gt 0) {
      $payloads.Add($encoding.GetBytes($text.Replace('Chromium', 'Saccade')))
      $replacementCount += $matches
    } else {
      $payload = New-Object byte[] $length
      [Array]::Copy($bytes, $start, $payload, 0, $length)
      $payloads.Add($payload)
    }
  }
  if ($replacementCount -eq 0) { return 0 }

  $temporary = "$Path.saccade.tmp"
  $stream = [IO.File]::Open($temporary, [IO.FileMode]::Create,
    [IO.FileAccess]::Write, [IO.FileShare]::None)
  $writer = New-Object IO.BinaryWriter $stream
  try {
    $writer.Write($bytes, 0, 12)
    [uint32]$nextOffset = $dataOffset
    for ($index = 0; $index -lt $resourceCount; $index++) {
      $writer.Write([uint16]$entries[$index].Id)
      $writer.Write($nextOffset)
      $nextOffset += [uint32]$payloads[$index].Length
    }
    $writer.Write([uint16]$entries[$resourceCount].Id)
    $writer.Write($nextOffset)
    $writer.Write($bytes, $aliasOffset, $aliasCount * 4)
    foreach ($payload in $payloads) { $writer.Write($payload) }
  } finally {
    $writer.Dispose()
    $stream.Dispose()
  }
  Move-Item -LiteralPath $temporary -Destination $Path -Force
  return $replacementCount
}

$total = 0
$changed = 0
foreach ($locale in Get-ChildItem -LiteralPath (Join-Path $PackageDir 'locales') -Filter '*.pak') {
  $count = Brand-LocalePack $locale.FullName
  if ($count -gt 0) {
    $total += $count
    $changed++
  }
}
Write-Output "Branded $total Chromium string occurrence(s) across $changed locale pack(s)."
