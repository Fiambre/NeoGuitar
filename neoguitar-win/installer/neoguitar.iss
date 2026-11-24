; NeoGuitar installer (Inno Setup 6).
; Build: iscc /DAppVersion=0.1.0 installer\neoguitar.iss   (creates dist\NeoGuitar-<version>-setup.exe)
; Requires a previous: cargo build --release

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

#define AppExe "neoguitar.exe"
#define ProgId "NeoGuitar.GuitarPro"

[Setup]
AppId={{6F1D2B7A-3C54-4E0B-9B52-0A6E7D2C91F4}
AppName=NeoGuitar
AppVersion={#AppVersion}
AppPublisher=Fiambre
AppPublisherURL=https://github.com/Fiambre/NeoGuitar
AppSupportURL=https://github.com/Fiambre/NeoGuitar/issues
AppUpdatesURL=https://github.com/Fiambre/NeoGuitar/releases
DefaultDirName={autopf}\NeoGuitar
DefaultGroupName=NeoGuitar
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=NeoGuitar-{#AppVersion}-setup
LicenseFile=..\..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Installs for the current user by default (no administrator rights); the wizard lets you choose all users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes
UninstallDisplayIcon={app}\{#AppExe}
SetupIconFile=..\assets\icon\neoguitar.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "assoc"; Description: "Associate Guitar Pro files (.gp3, .gp4, .gp5, .gpx, .gp) with NeoGuitar"; GroupDescription: "File associations:"
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "..\..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\soundfonts\*"; DestDir: "{app}\soundfonts"; Flags: ignoreversion recursesubdirs
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\NeoGuitar"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\NeoGuitar"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; HKA = HKCU for a per-user install, HKLM for all users.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Guitar Pro file"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExe},0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: assoc

Root: HKA; Subkey: "Software\Classes\.gp3"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp3\OpenWithProgids"; ValueType: none; ValueName: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp4"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp4\OpenWithProgids"; ValueType: none; ValueName: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp5"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp5\OpenWithProgids"; ValueType: none; ValueName: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gpx"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gpx\OpenWithProgids"; ValueType: none; ValueName: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.gp\OpenWithProgids"; ValueType: none; ValueName: "{#ProgId}"; Flags: uninsdeletevalue; Tasks: assoc

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch NeoGuitar"; Flags: nowait postinstall skipifsilent
