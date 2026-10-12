; NSIS 3 Unicode installer. Version defines come from packaging/version.py.
Unicode true
!ifndef HushVersion
  !error "HushVersion must be supplied by packaging/version.py"
!endif
!ifndef HushNumericVersion
  !error "HushNumericVersion must be supplied by packaging/version.py"
!endif
!define SourceRoot "../.."
!define AppId "io.hush.github"
!define UninstallKey "Software\Microsoft\Windows\CurrentVersion\Uninstall\${AppId}"
!define LegacyUninstallKey "${UninstallKey}_is1"
!define RunKey "Software\Microsoft\Windows\CurrentVersion\Run"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WinVer.nsh"
!include "x64.nsh"
!include "Win/COM.nsh"
!include "Win/Propkey.nsh"

Name "Hush ${HushVersion}"
OutFile "${SourceRoot}/dist/Hush-${HushVersion}-windows-x86_64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Hush"
RequestExecutionLevel user
ManifestSupportedOS Win10
SetCompressor /SOLID lzma
SetOverwrite on
ShowInstDetails show
ShowUninstDetails show
VIProductVersion "${HushNumericVersion}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Hush"
VIAddVersionKey /LANG=1033 "FileDescription" "Hush installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${HushVersion}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Hush contributors"
!define MUI_ICON "${SourceRoot}/assets/hush.ico"
!define MUI_UNICON "${SourceRoot}/assets/hush.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_LICENSE "${SourceRoot}/LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\hush.exe"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "German"

Var ShortcutPath
Var LegacyUninstaller

LangString CloseHush ${LANG_ENGLISH} "Quit Hush from its tray menu before continuing."
LangString CloseHush ${LANG_GERMAN} "Bitte Hush im Tray-Menü beenden, bevor Sie fortfahren."
LangString UnsupportedOS ${LANG_ENGLISH} "Hush requires 64-bit Windows 10 version 1809 or later."
LangString UnsupportedOS ${LANG_GERMAN} "Hush benötigt 64-Bit-Windows 10 ab Version 1809."
LangString DesktopShortcut ${LANG_ENGLISH} "Desktop shortcut"
LangString DesktopShortcut ${LANG_GERMAN} "Desktop-Verknüpfung"
LangString InstallFailed ${LANG_ENGLISH} "Hush setup could not complete. See the details below."
LangString InstallFailed ${LANG_GERMAN} "Hush-Setup konnte nicht abgeschlossen werden. Bitte die Details prüfen."

!macro RequireHushClosed
  ${If} ${FileExists} "$INSTDIR\hush.exe"
    ${Do}
      ClearErrors
      FileOpen $0 "$INSTDIR\hush.exe" a
      ${IfNot} ${Errors}
        FileClose $0
        ${ExitDo}
      ${EndIf}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(CloseHush)" /SD IDCANCEL IDRETRY +3
      SetErrorLevel 2
      Abort
    ${Loop}
  ${EndIf}
!macroend

!macro RequireSuccess
  ${If} ${Errors}
    MessageBox MB_OK|MB_ICONSTOP "$(InstallFailed)" /SD IDOK
    SetErrorLevel 2
    Abort
  ${EndIf}
!macroend

Function .onInit
  ${IfNot} ${RunningX64}
  ${OrIfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "$(UnsupportedOS)" /SD IDOK
    SetErrorLevel 2
    Quit
  ${EndIf}
  ${If} ${IsWin10}
  ${AndIfNot} ${AtLeastBuild} 17763
    MessageBox MB_OK|MB_ICONSTOP "$(UnsupportedOS)" /SD IDOK
    SetErrorLevel 2
    Quit
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  ; Keep existing installations, including releases from the previous packager,
  ; in their original directory. Never run the old uninstaller during upgrade:
  ; it would remove the user's autostart preference and notification identity.
  ReadRegStr $0 HKCU "${UninstallKey}" "InstallLocation"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "${LegacyUninstallKey}" "InstallLocation"
  ${EndIf}
  ${If} $0 != ""
    GetFullPathName $INSTDIR "$0"
    StrCpy $1 $INSTDIR 1 -1
    ${If} $1 == "\"
      StrCpy $INSTDIR $INSTDIR -1
    ${EndIf}
  ${EndIf}
  ReadRegStr $LegacyUninstaller HKCU "${LegacyUninstallKey}" "UninstallString"
FunctionEnd

; Create a real shortcut with PKEY_AppUserModel_ID, using only NSIS's bundled
; System plug-in and Windows COM. Notification identity matches the application.
Function CreateHushShortcut
  StrCpy $0 0
  StrCpy $1 0
  StrCpy $2 0
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ".r9"
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IShellLink::SetPath} $0 '("$INSTDIR\hush.exe").r9'
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IShellLink::SetWorkingDirectory} $0 '("$INSTDIR").r9'
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IShellLink::SetIconLocation} $0 '("$INSTDIR\hush.ico",0).r9'
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IUnknown::QueryInterface} $0 '("${IID_IPropertyStore}",.r1).r9'
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  System::Call '*(&w${NSIS_MAX_STRLEN} "${AppId}")p.r3'
  System::Call '*${SYSSTRUCT_PROPERTYKEY}(${PKEY_AppUserModel_ID})p.r4'
  System::Call '*${SYSSTRUCT_PROPVARIANT}(${VT_LPWSTR},,p r3)p.r5'
  ${IPropertyStore::SetValue} $1 '($4,$5).r9'
  System::Free $3
  System::Free $4
  System::Free $5
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IPropertyStore::Commit} $1 '.r9'
  ${If} $9 != 0
    Goto shortcut_cleanup
  ${EndIf}
  ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}",.r2).r9'
  ${If} $9 == 0
    ${IPersistFile::Save} $2 '("$ShortcutPath",1).r9'
  ${EndIf}
shortcut_cleanup:
  !insertmacro ComHlpr_SafeRelease $2
  !insertmacro ComHlpr_SafeRelease $1
  !insertmacro ComHlpr_SafeRelease $0
  ${If} $9 != 0
    DetailPrint "Cannot register shortcut $ShortcutPath (HRESULT $9)"
    MessageBox MB_OK|MB_ICONSTOP "$(InstallFailed)" /SD IDOK
    SetErrorLevel 2
    Abort
  ${EndIf}
FunctionEnd

Function RemoveLegacyUninstaller
  ; Only delete a registered unins*.exe and its data in this installation's
  ; directory. Do not use wildcards or recursively remove application folders.
  StrCpy $0 $LegacyUninstaller 1
  ${If} $0 == '$\"'
    StrCpy $LegacyUninstaller $LegacyUninstaller -1 1
  ${EndIf}
  ${GetParent} "$LegacyUninstaller" $0
  ${If} $0 == $INSTDIR
    ${GetFileName} "$LegacyUninstaller" $1
    StrCpy $2 $1 5
    StrCpy $3 $1 4 -4
    ${If} $2 == "unins"
    ${AndIf} $3 == ".exe"
      Delete "$LegacyUninstaller"
      StrCpy $1 $LegacyUninstaller -4
      Delete "$1.dat"
      Delete "$1.msg"
    ${EndIf}
    DeleteRegKey HKCU "${LegacyUninstallKey}"
  ${EndIf}
FunctionEnd

Section "Hush" MainSection
  SectionIn RO
  !insertmacro RequireHushClosed
  ClearErrors
  SetOutPath "$INSTDIR"
  File "${SourceRoot}/target/release/hush.exe"
  File "${SourceRoot}/assets/hush.ico"
  File "${SourceRoot}/LICENSE"
  SetOutPath "$INSTDIR\licenses\fonts"
  File "${SourceRoot}/assets/fonts/OFL.txt"
  File "${SourceRoot}/assets/fonts/README.md"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS"
  !insertmacro RequireSuccess
  StrCpy $ShortcutPath "$SMPROGRAMS\Hush.lnk"
  Call CreateHushShortcut
  ; Preserve an existing optional desktop shortcut during upgrades.
  ${If} ${FileExists} "$DESKTOP\Hush.lnk"
    StrCpy $ShortcutPath "$DESKTOP\Hush.lnk"
    Call CreateHushShortcut
  ${EndIf}
  ClearErrors
  WriteRegStr HKCU "Software\Classes\AppUserModelId\${AppId}" "DisplayName" "Hush"
  WriteRegStr HKCU "Software\Classes\AppUserModelId\${AppId}" "IconUri" "$INSTDIR\hush.ico"
  !insertmacro RequireSuccess
  ClearErrors
  ReadRegStr $0 HKCU "${RunKey}" "Hush"
  ${IfNot} ${Errors}
    WriteRegStr HKCU "${RunKey}" "Hush" '$\"$INSTDIR\hush.exe$\" --tray'
    !insertmacro RequireSuccess
  ${EndIf}
  ClearErrors
  WriteRegStr HKCU "${UninstallKey}" "DisplayName" "Hush"
  WriteRegStr HKCU "${UninstallKey}" "DisplayVersion" "${HushVersion}"
  WriteRegStr HKCU "${UninstallKey}" "Publisher" "Hush contributors"
  WriteRegStr HKCU "${UninstallKey}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UninstallKey}" "DisplayIcon" "$INSTDIR\hush.ico"
  WriteRegStr HKCU "${UninstallKey}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "${UninstallKey}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegDWORD HKCU "${UninstallKey}" "NoModify" 1
  WriteRegDWORD HKCU "${UninstallKey}" "NoRepair" 1
  !insertmacro RequireSuccess
  Call RemoveLegacyUninstaller
SectionEnd

Section /o "$(DesktopShortcut)" DesktopSection
  StrCpy $ShortcutPath "$DESKTOP\Hush.lnk"
  Call CreateHushShortcut
SectionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
FunctionEnd

Section "Uninstall"
  !insertmacro RequireHushClosed
  ClearErrors
  Delete "$INSTDIR\hush.exe"
  !insertmacro RequireSuccess
  Delete "$SMPROGRAMS\Hush.lnk"
  Delete "$DESKTOP\Hush.lnk"
  ; Remove startup even if it was enabled from Hush after installation.
  DeleteRegValue HKCU "${RunKey}" "Hush"
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\${AppId}"
  DeleteRegKey HKCU "${UninstallKey}"
  Delete "$INSTDIR\hush.ico"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\licenses\fonts\OFL.txt"
  Delete "$INSTDIR\licenses\fonts\README.md"
  RMDir "$INSTDIR\licenses\fonts"
  RMDir "$INSTDIR\licenses"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  ; Account data, keyring credentials, history, and unrelated files are retained.
SectionEnd
