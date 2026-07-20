[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PackageDir,
  [Parameter(Mandatory = $true)][string]$CertificateThumbprint,
  [string]$TimestampUrl = 'http://timestamp.digicert.com',
  [string]$SignTool = ''
)

$ErrorActionPreference = 'Stop'
$resolvedPackage = (Resolve-Path -LiteralPath $PackageDir).Path
$thumbprint = $CertificateThumbprint.Replace(' ', '').ToUpperInvariant()
$certificate = Get-ChildItem Cert:\CurrentUser\My,Cert:\LocalMachine\My -CodeSigningCert |
  Where-Object { $_.Thumbprint -eq $thumbprint -and $_.HasPrivateKey } |
  Select-Object -First 1
if (-not $certificate) {
  throw "No code-signing certificate with private key found for thumbprint $thumbprint"
}
if ($certificate.NotAfter -le (Get-Date)) {
  throw "Code-signing certificate $thumbprint has expired"
}
if ($certificate.PublicKey.Oid.FriendlyName -notmatch 'RSA') {
  throw 'Smart App Control requires an RSA code-signing certificate; ECC is not supported'
}
if (-not $SignTool) {
  $SignTool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' `
    -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $SignTool -or -not (Test-Path -LiteralPath $SignTool)) {
  throw 'signtool.exe was not found in the Windows SDK'
}

$binaries = Get-ChildItem -LiteralPath $resolvedPackage -File |
  Where-Object { $_.Extension -in @('.exe', '.dll') }
foreach ($binary in $binaries) {
  $signature = Get-AuthenticodeSignature -LiteralPath $binary.FullName
  if ($signature.Status -eq 'Valid') { continue }
  & $SignTool sign /sha1 $thumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 `
    /d 'Saccade dogfood browser' $binary.FullName
  if ($LASTEXITCODE -ne 0) { throw "Signing failed: $($binary.FullName)" }
  & $SignTool verify /pa /v $binary.FullName
  if ($LASTEXITCODE -ne 0) { throw "Signature verification failed: $($binary.FullName)" }
}

Write-Host "Signed $($binaries.Count) Saccade executable component(s) with $thumbprint"
