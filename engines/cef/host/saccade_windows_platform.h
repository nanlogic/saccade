// Copyright (c) 2026 Saccade contributors.

#ifndef SACCADE_CEF_HOST_SACCADE_WINDOWS_PLATFORM_H_
#define SACCADE_CEF_HOST_SACCADE_WINDOWS_PLATFORM_H_

#if !defined(OS_WIN)
#error This helper is Windows-only.
#endif

#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <string>

using SaccadeSSize = SSIZE_T;

constexpr int kSaccadeOpenReadOnly = 0x0000;
constexpr int kSaccadeOpenWriteOnly = 0x0001;
constexpr int kSaccadeOpenCreate = 0x0100;
constexpr int kSaccadeOpenTruncate = 0x0200;
constexpr int kSaccadeOpenAppend = 0x0008;
constexpr int kSaccadeOpenNoFollow = 0;

bool SaccadeRandomBytes(void* output, size_t size);
int SaccadeOpen(const char* path, int flags, ...);
SaccadeSSize SaccadeRead(int descriptor, void* output, size_t size);
SaccadeSSize SaccadeWrite(int descriptor, const void* input, size_t size);
int SaccadeClose(int descriptor);
int SaccadeCommit(int descriptor);
int SaccadeChmodOwnerOnly(int descriptor, int mode);
int SaccadeUnlink(const char* path);
int SaccadeRenameReplace(const char* source, const char* destination);
int SaccadeProcessId();

int SaccadeCreateNamedPipeListener(const std::string& pipe_name);
int SaccadeAcceptNamedPipe(int listener_descriptor);
void SaccadeShutdownNamedPipe(int listener_descriptor);

bool SaccadeEnsureOwnerOnlyDirectory(const std::wstring& path);
bool SaccadeApplyOwnerOnlyDacl(const std::wstring& path);
bool SaccadeRemovePointerIfOwned(const std::string& pointer_path,
                                 const std::string& expected_contents);

#endif  // SACCADE_CEF_HOST_SACCADE_WINDOWS_PLATFORM_H_
