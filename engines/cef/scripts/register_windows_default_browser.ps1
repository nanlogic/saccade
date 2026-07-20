[CmdletBinding()]
param(
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\Saccade'),
  [switch]$OpenSettings
)

$ErrorActionPreference = 'Stop'
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$exe = Join-Path $InstallDir 'Saccade.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "Missing $exe" }
$icon = "`"$exe`",0"
$openCommand = "`"$exe`" --url=`"%1`""

$client = 'HKCU:\Software\Clients\StartMenuInternet\Saccade'
$capabilities = Join-Path $client 'Capabilities'
New-Item -Path $client -Force | Out-Null
Set-Item -Path $client -Value 'Saccade'
New-Item -Path (Join-Path $client 'DefaultIcon') -Force | Out-Null
Set-Item -Path (Join-Path $client 'DefaultIcon') -Value $icon
New-Item -Path (Join-Path $client 'shell\open\command') -Force | Out-Null
Set-Item -Path (Join-Path $client 'shell\open\command') -Value "`"$exe`""
New-Item -Path $capabilities -Force | Out-Null
New-ItemProperty -Path $capabilities -Name ApplicationName -Value 'Saccade' -PropertyType String -Force | Out-Null
New-ItemProperty -Path $capabilities -Name ApplicationDescription -Value 'Saccade dogfood browser' -PropertyType String -Force | Out-Null
New-ItemProperty -Path $capabilities -Name ApplicationIcon -Value $icon -PropertyType String -Force | Out-Null

$urlAssociations = Join-Path $capabilities 'URLAssociations'
New-Item -Path $urlAssociations -Force | Out-Null
foreach ($protocol in 'http', 'https') {
  New-ItemProperty -Path $urlAssociations -Name $protocol -Value 'SaccadeURL' -PropertyType String -Force | Out-Null
}
$fileAssociations = Join-Path $capabilities 'FileAssociations'
New-Item -Path $fileAssociations -Force | Out-Null
foreach ($extension in '.htm', '.html', '.shtml', '.xht', '.xhtml') {
  New-ItemProperty -Path $fileAssociations -Name $extension -Value 'SaccadeHTML' -PropertyType String -Force | Out-Null
}

foreach ($class in 'SaccadeURL', 'SaccadeHTML') {
  $classPath = "HKCU:\Software\Classes\$class"
  New-Item -Path $classPath -Force | Out-Null
  Set-Item -Path $classPath -Value 'Saccade browser document'
  if ($class -eq 'SaccadeURL') {
    New-ItemProperty -Path $classPath -Name 'URL Protocol' -Value '' -PropertyType String -Force | Out-Null
  }
  New-Item -Path (Join-Path $classPath 'DefaultIcon') -Force | Out-Null
  Set-Item -Path (Join-Path $classPath 'DefaultIcon') -Value $icon
  New-Item -Path (Join-Path $classPath 'shell\open\command') -Force | Out-Null
  Set-Item -Path (Join-Path $classPath 'shell\open\command') -Value $openCommand
}

$registered = 'HKCU:\Software\RegisteredApplications'
New-Item -Path $registered -Force | Out-Null
New-ItemProperty -Path $registered -Name Saccade -Value 'Software\Clients\StartMenuInternet\Saccade\Capabilities' -PropertyType String -Force | Out-Null

if ($OpenSettings) {
  Start-Process 'ms-settings:defaultapps?registeredAppUser=Saccade'
}
[pscustomobject]@{ Registered = $true; Application = 'Saccade'; Executable = $exe; SettingsOpened = [bool]$OpenSettings }
