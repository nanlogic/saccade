[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")

if (-not $text.Contains('#include <cstdlib>')) {
  $marker = "#include <array>`n"
  if (-not $text.Contains($marker)) { throw 'Direct-session include marker missing' }
  $text = $text.Replace($marker, $marker + "#include <cstdlib>`n")
}

$old = @"
bool SetEnvironment(const wchar_t* name, const std::wstring& value) {
  return SetEnvironmentVariableW(name, value.c_str()) != FALSE;
}
"@
$new = @"
bool SetEnvironment(const wchar_t* name, const std::wstring& value) {
  // Keep the CRT environment used by getenv() and the Win32 environment
  // inherited by CEF subprocesses in sync.
  return _wputenv_s(name, value.c_str()) == 0 &&
         SetEnvironmentVariableW(name, value.c_str()) != FALSE;
}
"@
if ($text.Contains($old)) {
  $text = $text.Replace($old, $new)
} elseif (-not $text.Contains('_wputenv_s(name, value.c_str())')) {
  throw 'Direct-session SetEnvironment function was not recognized'
}

[IO.File]::WriteAllText($resolved, $text, [Text.UTF8Encoding]::new($false))
