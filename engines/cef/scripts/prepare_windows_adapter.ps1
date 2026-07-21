[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$Destination
)

$ErrorActionPreference = 'Stop'
$text = [System.IO.File]::ReadAllText((Resolve-Path $Source))

function Replace-Exact {
  param(
    [Parameter(Mandatory = $true)][string]$Old,
    [Parameter(Mandatory = $true)][string]$New
  )
  $Old = $Old.Replace("`r`n", "`n")
  $New = $New.Replace("`r`n", "`n")
  if (-not $script:text.Contains($Old)) {
    throw "Windows adapter transform lost expected Build 64 source fragment: $Old"
  }
  $script:text = $script:text.Replace($Old, $New)
}

Replace-Exact @'
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>
'@ @'
#if defined(OS_WIN)
#include "tests/cefsimple/saccade_windows_platform.h"
#else
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>
#endif
'@

Replace-Exact @'
#if defined(OS_MAC)
#include "tests/cefsimple/saccade_agent_switch_mac.h"
#endif

namespace {
'@ @'
#if defined(OS_MAC)
#include "tests/cefsimple/saccade_agent_switch_mac.h"
#elif defined(OS_WIN)
#include "tests/cefsimple/saccade_agent_switch_win.h"
#endif

#if defined(OS_WIN)
#define arc4random_buf(output, size) SaccadeRandomBytes((output), (size))
#define open SaccadeOpen
#define read SaccadeRead
#define write SaccadeWrite
#define close SaccadeClose
#define fsync SaccadeCommit
#define fchmod SaccadeChmodOwnerOnly
#define unlink SaccadeUnlink
#define rename SaccadeRenameReplace
#define getpid SaccadeProcessId
#define ssize_t SaccadeSSize
#define O_RDONLY kSaccadeOpenReadOnly
#define O_WRONLY kSaccadeOpenWriteOnly
#define O_CREAT kSaccadeOpenCreate
#define O_TRUNC kSaccadeOpenTruncate
#define O_APPEND kSaccadeOpenAppend
#define O_NOFOLLOW kSaccadeOpenNoFollow
#endif

namespace {
'@

$text = $text.Replace('#if defined(OS_MAC)', '#if defined(OS_MAC) || defined(OS_WIN)')
$text = $text.Replace(
  '#if defined(OS_MAC) || defined(OS_WIN)' + "`r`n" +
  '#include "tests/cefsimple/saccade_agent_switch_mac.h"' + "`r`n" +
  '#elif defined(OS_WIN)',
  '#if defined(OS_MAC)' + "`r`n" +
  '#include "tests/cefsimple/saccade_agent_switch_mac.h"' + "`r`n" +
  '#elif defined(OS_WIN)')
$text = $text.Replace(
  '#if defined(OS_MAC) || defined(OS_WIN)' + "`n" +
  '#include "tests/cefsimple/saccade_agent_switch_mac.h"' + "`n" +
  '#elif defined(OS_WIN)',
  '#if defined(OS_MAC)' + "`n" +
  '#include "tests/cefsimple/saccade_agent_switch_mac.h"' + "`n" +
  '#elif defined(OS_WIN)')

Replace-Exact @'
  if (listener >= 0) {
    shutdown(listener, SHUT_RDWR);
    close(listener);
  }
'@ @'
  if (listener >= 0) {
#if defined(OS_WIN)
    SaccadeShutdownNamedPipe(listener);
#else
    shutdown(listener, SHUT_RDWR);
#endif
    close(listener);
  }
'@

Replace-Exact @'
  if (!socket_path_.empty()) {
    unlink(socket_path_.c_str());
  }
  if (!grant_path_.empty()) {
'@ @'
#if !defined(OS_WIN)
  if (!socket_path_.empty()) {
    unlink(socket_path_.c_str());
  }
#endif
  if (!grant_path_.empty()) {
'@

Replace-Exact @'
void SaccadeAdapter::Serve() {
  unlink(socket_path_.c_str());
'@ @'
void SaccadeAdapter::Serve() {
#if defined(OS_WIN)
  const int listener = SaccadeCreateNamedPipeListener(socket_path_);
  if (listener < 0) {
    return;
  }
  listener_fd_ = listener;
  if (!WriteGrant()) {
    close(listener);
    listener_fd_ = -1;
    return;
  }
#else
  unlink(socket_path_.c_str());
'@

Replace-Exact @'
  while (!stopping_) {
    const int client = accept(listener, nullptr, nullptr);
'@ @'
#endif
  while (!stopping_) {
#if defined(OS_WIN)
    const int client = SaccadeAcceptNamedPipe(listener);
#else
    const int client = accept(listener, nullptr, nullptr);
#endif
'@

Replace-Exact @'
  endpoint->SetString("scheme", "unix");
'@ @'
#if defined(OS_WIN)
  endpoint->SetString("scheme", "windows_named_pipe");
#else
  endpoint->SetString("scheme", "unix");
#endif
'@

Replace-Exact @'
  adapter->SetString("transport", "owner_only_unix_v1");
'@ @'
#if defined(OS_WIN)
  adapter->SetString("transport", "owner_only_windows_pipe_v1");
#else
  adapter->SetString("transport", "owner_only_unix_v1");
#endif
'@

Replace-Exact @'
  const int fd = open(current_pointer_path_.c_str(), O_RDONLY | O_NOFOLLOW);
'@ @'
#if defined(OS_WIN)
  SaccadeRemovePointerIfOwned(current_pointer_path_, grant_path_ + "\n");
  return;
#else
  const int fd = open(current_pointer_path_.c_str(), O_RDONLY | O_NOFOLLOW);
'@

Replace-Exact @'
    unlink(current_pointer_path_.c_str());
  }
}

void SaccadeAdapter::ResetPageStateLocked
'@ @'
    unlink(current_pointer_path_.c_str());
  }
#endif
}

void SaccadeAdapter::ResetPageStateLocked
'@

$destinationDirectory = Split-Path -Parent $Destination
New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
[System.IO.File]::WriteAllText($Destination, $text,
  [System.Text.UTF8Encoding]::new($false))
