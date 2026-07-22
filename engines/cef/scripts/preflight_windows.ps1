[CmdletBinding()]
param(
    [switch]$FetchCef
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "This preflight must run on Windows 10/11 x64."
}
if (-not [Environment]::Is64BitOperatingSystem) {
    throw "Saccade Windows dogfood currently requires Windows x64."
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path
$Missing = [System.Collections.Generic.List[string]]::new()

foreach ($Command in @("git.exe", "cmake.exe", "python.exe", "cargo.exe", "rustc.exe")) {
    if ($null -eq (Get-Command $Command -ErrorAction SilentlyContinue)) {
        $Missing.Add($Command)
    }
}

$VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$VsPath = $null
if (Test-Path -LiteralPath $VsWhere -PathType Leaf) {
    $VsPath = & $VsWhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath
}
if ([string]::IsNullOrWhiteSpace($VsPath)) {
    $Missing.Add("Visual Studio 2022 Desktop development with C++")
}

if ($Missing.Count -gt 0) {
    Write-Error ("WINDOWS_PREFLIGHT FAIL missing=" + ($Missing -join ", "))
    exit 1
}

$HostArchitecture = & rustc.exe -vV |
    Select-String '^host:' |
    ForEach-Object { $_.Line.Split(':', 2)[1].Trim() }
if ($HostArchitecture -ne "x86_64-pc-windows-msvc") {
    Write-Error "WINDOWS_PREFLIGHT FAIL rust_host=${HostArchitecture} expected=x86_64-pc-windows-msvc"
    exit 1
}

$CefRoot = "not fetched"
if ($FetchCef) {
    $CefRoot = & (Join-Path $ScriptDir "fetch_windows.ps1")
}

Write-Host "WINDOWS_PREFLIGHT PASS"
Write-Host "repo=$RepoRoot"
Write-Host "visual_studio=$VsPath"
Write-Host "rust_host=$HostArchitecture"
Write-Host "cef_root=$CefRoot"
Write-Host "next=run the Build 79 rebuild or regression gate from docs/windows_dogfood_quickstart.md"
