# Per-user installation. Explicitly registers the app's own notification identity.
# Run in PowerShell after building; no admin rights or machine-wide policy changes.
param([string]$Binary = "$PSScriptRoot\..\..\target\release\hush.exe",[switch]$AutoStart)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "Binary missing: $Binary. Run cargo build --release first." }
$Root = (Resolve-Path "$PSScriptRoot\..\..").Path
$Destination = Join-Path $env:LOCALAPPDATA 'Programs\Hush'
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $Destination 'hush.exe') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\hush.ico') -Destination $Destination -Force
$FontLicenses = Join-Path $Destination 'licenses\fonts'
New-Item -ItemType Directory -Path $FontLicenses -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Root 'LICENSE') -Destination $Destination -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\fonts\OFL.txt') -Destination $FontLicenses -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\fonts\README.md') -Destination $FontLicenses -Force
$Executable = Join-Path $Destination 'hush.exe'
$Icon = Join-Path $Destination 'hush.ico'
$AppId = 'io.hush.github'
$Key = "HKCU:\Software\Classes\AppUserModelId\$AppId"
New-Item -Path $Key -Force | Out-Null
New-ItemProperty -Path $Key -Name DisplayName -Value 'Hush' -PropertyType String -Force | Out-Null
New-ItemProperty -Path $Key -Name IconUri -Value $Icon -PropertyType String -Force | Out-Null
# A real Start Menu shortcut with PKEY_AppUserModel_ID; not just a filename or
# an impersonated Explorer/PowerShell app id. COM is used only by this installer.
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
public static class HushShortcut {
 [ComImport, Guid("00021401-0000-0000-C000-000000000046")] class ShellLink {}
 [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("000214F9-0000-0000-C000-000000000046")]
 interface IShellLinkW {
  void GetPath([Out,MarshalAs(UnmanagedType.LPWStr)] StringBuilder p,int c,IntPtr fd,uint f);
  void GetIDList(out IntPtr p); void SetIDList(IntPtr p);
  void GetDescription([Out,MarshalAs(UnmanagedType.LPWStr)] StringBuilder s,int c);
  void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string s);
  void GetWorkingDirectory([Out,MarshalAs(UnmanagedType.LPWStr)] StringBuilder s,int c);
  void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string s);
  void GetArguments([Out,MarshalAs(UnmanagedType.LPWStr)] StringBuilder s,int c);
  void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string s);
  void GetHotkey(out short w); void SetHotkey(short w); void GetShowCmd(out int i); void SetShowCmd(int i);
  void GetIconLocation([Out,MarshalAs(UnmanagedType.LPWStr)] StringBuilder s,int c,out int i);
  void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string s,int i);
  void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string s,uint r); void Resolve(IntPtr w,uint f);
  void SetPath([MarshalAs(UnmanagedType.LPWStr)] string s);
 }
 [StructLayout(LayoutKind.Sequential,Pack=4)] struct PropertyKey { public Guid fmtid; public uint pid; }
 [StructLayout(LayoutKind.Explicit)] struct PropVariant { [FieldOffset(0)] public ushort type; [FieldOffset(8)] public IntPtr value; }
 [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99")]
 interface IPropertyStore {
  [PreserveSig] int GetCount(out uint c); [PreserveSig] int GetAt(uint i,out PropertyKey k);
  [PreserveSig] int GetValue(ref PropertyKey k,out PropVariant v);
  [PreserveSig] int SetValue(ref PropertyKey k,ref PropVariant v); [PreserveSig] int Commit();
 }
 public static void Create(string file,string exe,string icon,string appId) {
  object link=new ShellLink(); IntPtr value=IntPtr.Zero;
  try {
   IShellLinkW s=(IShellLinkW)link; s.SetPath(exe);s.SetWorkingDirectory(System.IO.Path.GetDirectoryName(exe));
   s.SetDescription("Hush – GitHub notifications");s.SetIconLocation(icon,0);
   PropertyKey key=new PropertyKey{fmtid=new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"),pid=5};
   value=Marshal.StringToCoTaskMemUni(appId);PropVariant variant=new PropVariant{type=31,value=value};
   IPropertyStore store=(IPropertyStore)link;Marshal.ThrowExceptionForHR(store.SetValue(ref key,ref variant));Marshal.ThrowExceptionForHR(store.Commit());
   ((IPersistFile)link).Save(file,true);
  } finally {if(value!=IntPtr.Zero)Marshal.FreeCoTaskMem(value);if(Marshal.IsComObject(link))Marshal.FinalReleaseComObject(link);}
 }
}
'@
$StartMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Hush.lnk'
[HushShortcut]::Create($StartMenu,$Executable,$Icon,$AppId)
if ($AutoStart) {
 $RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
 New-Item -Path $RunKey -Force | Out-Null
 New-ItemProperty -Path $RunKey -Name 'Hush' -Value ('"' + $Executable + '" --tray') -PropertyType String -Force | Out-Null
}
Write-Host "Hush installed: $Executable"
Write-Host 'Open Hush from the Start menu. Allow notifications in Windows Settings > System > Notifications.'
Write-Host 'Choose Start at login in Hush settings. -AutoStart also enables it explicitly; otherwise this script preserves the existing choice.'
Write-Host 'To uninstall, disable Start at login, disconnect the account if needed, and remove the installation, Hush.lnk, and the Hush AppUserModelID registry entry.'
