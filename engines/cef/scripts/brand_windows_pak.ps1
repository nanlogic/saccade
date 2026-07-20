[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PackageDir,
  [Parameter(Mandatory = $true)][string]$Icon
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$PackageDir = (Resolve-Path -LiteralPath $PackageDir).Path
$Icon = (Resolve-Path -LiteralPath $Icon).Path
$faviconIds = @(477, 49469, 49470, 49471, 49472, 49473)

function Convert-IconToPng([int]$Size) {
  $frame = New-Object System.Drawing.Icon $Icon, $Size, $Size
  try {
    $bitmap = $frame.ToBitmap()
    try {
      $stream = New-Object System.IO.MemoryStream
      try {
        $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
        return $stream.ToArray()
      } finally {
        $stream.Dispose()
      }
    } finally {
      $bitmap.Dispose()
    }
  } finally {
    $frame.Dispose()
  }
}

function Get-PngDimension([byte[]]$Bytes, [int]$Offset) {
  if ($Bytes[$Offset] -ne 0x89 -or $Bytes[$Offset + 1] -ne 0x50) {
    throw 'Expected a PNG favicon resource'
  }
  $width = ([uint32]$Bytes[$Offset + 16] -shl 24) -bor
    ([uint32]$Bytes[$Offset + 17] -shl 16) -bor
    ([uint32]$Bytes[$Offset + 18] -shl 8) -bor
    [uint32]$Bytes[$Offset + 19]
  $height = ([uint32]$Bytes[$Offset + 20] -shl 24) -bor
    ([uint32]$Bytes[$Offset + 21] -shl 16) -bor
    ([uint32]$Bytes[$Offset + 22] -shl 8) -bor
    [uint32]$Bytes[$Offset + 23]
  if ($width -ne $height -or $width -notin 16, 32, 64, 128) {
    throw "Unexpected favicon dimensions ${width}x${height}"
  }
  return [int]$width
}

function Brand-DataPack([string]$Path) {
  [byte[]]$bytes = [System.IO.File]::ReadAllBytes($Path)
  if ([BitConverter]::ToUInt32($bytes, 0) -ne 5) {
    throw "Unsupported Chromium DataPack version in $Path"
  }
  $resourceCount = [BitConverter]::ToUInt16($bytes, 8)
  $aliasCount = [BitConverter]::ToUInt16($bytes, 10)
  $entries = @()
  for ($index = 0; $index -le $resourceCount; $index++) {
    $entryOffset = 12 + ($index * 6)
    $entries += [pscustomobject]@{
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
  $replaced = 0
  for ($index = 0; $index -lt $resourceCount; $index++) {
    $start = [int]$entries[$index].Offset
    $end = [int]$entries[$index + 1].Offset
    $length = $end - $start
    if ($faviconIds -contains [int]$entries[$index].Id) {
      $size = Get-PngDimension $bytes $start
      $payloads.Add((Convert-IconToPng $size))
      $replaced++
    } else {
      $payload = New-Object byte[] $length
      [Array]::Copy($bytes, $start, $payload, 0, $length)
      $payloads.Add($payload)
    }
  }
  if ($replaced -ne $faviconIds.Count) {
    throw "Expected $($faviconIds.Count) favicon resources in $Path, replaced $replaced"
  }

  $temporary = "$Path.saccade.tmp"
  $stream = [System.IO.File]::Open($temporary, [System.IO.FileMode]::Create,
    [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
  $writer = New-Object System.IO.BinaryWriter $stream
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
  Write-Output "Branded $([IO.Path]::GetFileName($Path)): $replaced favicon resources"
}

Brand-DataPack (Join-Path $PackageDir 'chrome_100_percent.pak')
Brand-DataPack (Join-Path $PackageDir 'chrome_200_percent.pak')
