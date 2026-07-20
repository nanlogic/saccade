[CmdletBinding()]
param([string]$Path = (Join-Path $PSScriptRoot '..\crates\saccade_engine_api\src\lib.rs'))

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path $Path).Path
$text = [System.IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")
$text = $text.Replace(
  '            if !path.is_absolute() {',
  '            if !path.is_absolute() && !path.to_string_lossy().starts_with(''/'') {')
$text = $text.Replace(
  '    use std::fs::OpenOptions;' + "`n" + '    #[cfg(unix)]',
  '    #[cfg(unix)]' + "`n" + '    use std::fs::OpenOptions;' + "`n" + '    #[cfg(unix)]')
[System.IO.File]::WriteAllText($resolved, $text,
  [System.Text.UTF8Encoding]::new($false))
