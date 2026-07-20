[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$path = Join-Path $repoRoot 'engines\cef\scripts\register_windows_default_browser.ps1'
$source = Get-Content -LiteralPath $path -Raw
$oldProtocols = @'
foreach ($protocol in 'http', 'https') {
  New-Item -Path (Join-Path $capabilities 'URLAssociations') -Force | Out-Null
  New-ItemProperty -Path (Join-Path $capabilities 'URLAssociations') -Name $protocol -Value 'SaccadeURL' -PropertyType String -Force | Out-Null
}
'@
$newProtocols = @'
$urlAssociations = Join-Path $capabilities 'URLAssociations'
New-Item -Path $urlAssociations -Force | Out-Null
foreach ($protocol in 'http', 'https') {
  New-ItemProperty -Path $urlAssociations -Name $protocol -Value 'SaccadeURL' -PropertyType String -Force | Out-Null
}
'@
$oldFiles = @'
foreach ($extension in '.htm', '.html', '.shtml', '.xht', '.xhtml') {
  New-Item -Path (Join-Path $capabilities 'FileAssociations') -Force | Out-Null
  New-ItemProperty -Path (Join-Path $capabilities 'FileAssociations') -Name $extension -Value 'SaccadeHTML' -PropertyType String -Force | Out-Null
}
'@
$newFiles = @'
$fileAssociations = Join-Path $capabilities 'FileAssociations'
New-Item -Path $fileAssociations -Force | Out-Null
foreach ($extension in '.htm', '.html', '.shtml', '.xht', '.xhtml') {
  New-ItemProperty -Path $fileAssociations -Name $extension -Value 'SaccadeHTML' -PropertyType String -Force | Out-Null
}
'@
if (-not $source.Contains($oldProtocols.Trim())) { throw 'Protocol association block not found' }
if (-not $source.Contains($oldFiles.Trim())) { throw 'File association block not found' }
$source = $source.Replace($oldProtocols.Trim(), $newProtocols.Trim())
$source = $source.Replace($oldFiles.Trim(), $newFiles.Trim())
Set-Content -LiteralPath $path -Value $source -Encoding utf8
Write-Output 'Fixed Windows default-browser association registration.'
