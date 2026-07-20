[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Executable,
  [Parameter(Mandatory = $true)][string]$Icon
)

$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$Icon = (Resolve-Path -LiteralPath $Icon).Path

if (-not ('Saccade.WindowsIconBranding' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

namespace Saccade {
  public static class WindowsIconBranding {
    private const int RT_ICON = 3;
    private const int RT_GROUP_ICON = 14;
    private const int IDI_APPLICATION = 32512;

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr BeginUpdateResource(string fileName, bool deleteExistingResources);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool UpdateResource(
        IntPtr update,
        IntPtr type,
        IntPtr name,
        ushort language,
        byte[] data,
        uint dataSize);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool EndUpdateResource(IntPtr update, bool discard);

    private static ushort ReadUInt16(byte[] data, int offset) {
      return BitConverter.ToUInt16(data, offset);
    }

    private static uint ReadUInt32(byte[] data, int offset) {
      return BitConverter.ToUInt32(data, offset);
    }

    private static void WriteUInt16(byte[] data, int offset, ushort value) {
      byte[] encoded = BitConverter.GetBytes(value);
      Buffer.BlockCopy(encoded, 0, data, offset, encoded.Length);
    }

    private static void WriteUInt32(byte[] data, int offset, uint value) {
      byte[] encoded = BitConverter.GetBytes(value);
      Buffer.BlockCopy(encoded, 0, data, offset, encoded.Length);
    }

    private static void Check(bool result, string operation) {
      if (!result) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), operation);
      }
    }

    public static int Apply(string executablePath, string iconPath) {
      byte[] ico = File.ReadAllBytes(iconPath);
      if (ico.Length < 6 || ReadUInt16(ico, 0) != 0 || ReadUInt16(ico, 2) != 1) {
        throw new InvalidDataException("Expected a Windows .ico file.");
      }

      ushort count = ReadUInt16(ico, 4);
      if (count == 0 || ico.Length < 6 + count * 16) {
        throw new InvalidDataException("The .ico directory is incomplete.");
      }

      byte[] group = new byte[6 + count * 14];
      WriteUInt16(group, 0, 0);
      WriteUInt16(group, 2, 1);
      WriteUInt16(group, 4, count);

      IntPtr update = BeginUpdateResource(executablePath, false);
      if (update == IntPtr.Zero) {
        throw new Win32Exception(Marshal.GetLastWin32Error(), "BeginUpdateResource failed");
      }

      bool committed = false;
      try {
        for (int i = 0; i < count; i++) {
          int sourceEntry = 6 + i * 16;
          int groupEntry = 6 + i * 14;
          uint imageSize = ReadUInt32(ico, sourceEntry + 8);
          uint imageOffset = ReadUInt32(ico, sourceEntry + 12);
          if (imageSize == 0 || imageOffset + imageSize > ico.Length) {
            throw new InvalidDataException("The .ico image data is incomplete.");
          }

          ushort resourceId = (ushort)(5000 + i);
          byte[] image = new byte[imageSize];
          Buffer.BlockCopy(ico, (int)imageOffset, image, 0, (int)imageSize);
          Buffer.BlockCopy(ico, sourceEntry, group, groupEntry, 8);
          WriteUInt32(group, groupEntry + 8, imageSize);
          WriteUInt16(group, groupEntry + 12, resourceId);

          foreach (ushort language in new ushort[] { 0, 1033 }) {
            Check(UpdateResource(update, (IntPtr)RT_ICON, (IntPtr)resourceId,
                                 language, image, imageSize),
                  "Updating icon image failed");
          }
        }

        foreach (ushort language in new ushort[] { 0, 1033 }) {
          Check(UpdateResource(update, (IntPtr)RT_GROUP_ICON,
                               (IntPtr)IDI_APPLICATION, language, group,
                               (uint)group.Length),
                "Updating application icon failed");
        }

        Check(EndUpdateResource(update, false), "EndUpdateResource failed");
        committed = true;
        return count;
      } finally {
        if (!committed) {
          EndUpdateResource(update, true);
        }
      }
    }
  }
}
'@
}

$frameCount = [Saccade.WindowsIconBranding]::Apply($Executable, $Icon)
Write-Host "Applied $frameCount Saccade icon sizes to $Executable"
