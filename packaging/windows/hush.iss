; Compile from the repository root with /DHushVersion and /DHushNumericVersion.
#ifndef HushVersion
  #error HushVersion must be supplied by packaging/version.py
#endif
#ifndef HushNumericVersion
  #error HushNumericVersion must be supplied by packaging/version.py
#endif
#define SourceRoot "..\.."

[Setup]
AppId=io.hush.github
AppName=Hush
AppVersion={#HushVersion}
AppVerName=Hush {#HushVersion}
VersionInfoVersion={#HushNumericVersion}.0
AppPublisher=Hush contributors
DefaultDirName={localappdata}\Programs\Hush
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
DisableProgramGroupPage=yes
LicenseFile={#SourceRoot}\LICENSE
SetupIconFile={#SourceRoot}\assets\hush.ico
UninstallDisplayIcon={app}\hush.ico
OutputDir={#SourceRoot}\dist
OutputBaseFilename=Hush-{#HushVersion}-windows-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceRoot}\target\release\hush.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\assets\hush.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\assets\fonts\OFL.txt"; DestDir: "{app}\licenses\fonts"; Flags: ignoreversion
Source: "{#SourceRoot}\assets\fonts\README.md"; DestDir: "{app}\licenses\fonts"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Hush"; Filename: "{app}\hush.exe"; IconFilename: "{app}\hush.ico"; AppUserModelID: "io.hush.github"
Name: "{userdesktop}\Hush"; Filename: "{app}\hush.exe"; IconFilename: "{app}\hush.ico"; AppUserModelID: "io.hush.github"; Tasks: desktopicon

[Registry]
; Register uninstall cleanup even when autostart is enabled later from Hush.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Hush"; Flags: dontcreatekey uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\io.hush.github"; ValueType: string; ValueName: "DisplayName"; ValueData: "Hush"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\io.hush.github"; ValueType: string; ValueName: "IconUri"; ValueData: "{app}\hush.ico"
; Preserve the current opt-in across upgrades and refresh its executable path.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Hush"; ValueData: """{app}\hush.exe"" --tray"; Check: RegValueExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Hush'); Flags: uninsdeletevalue

[Run]
Filename: "{app}\hush.exe"; Description: "Launch Hush"; Flags: nowait postinstall skipifsilent

; Application data and keyring entries are retained. Disconnect the account
; inside Hush before uninstalling if those should also be removed.
