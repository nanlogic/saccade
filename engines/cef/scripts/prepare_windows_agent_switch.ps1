[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Destination
)

$ErrorActionPreference = 'Stop'
$text = [System.IO.File]::ReadAllText((Resolve-Path $Source))
$text = $text.Replace(
  'const int x = std::max(8, bounds.right - right_reserve - width);',
  'const int x = static_cast<int>(std::max<LONG>(' +
    '8, bounds.right - right_reserve - width));')
foreach ($id in @('kPromptEdit', 'kPromptFill', 'kPromptCancel')) {
  $text = $text.Replace(
    "reinterpret_cast<HMENU>($id)",
    "reinterpret_cast<HMENU>(static_cast<INT_PTR>($id))")
}
[System.IO.File]::WriteAllText($Destination, $text,
  [System.Text.UTF8Encoding]::new($false))
