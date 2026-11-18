; Instalador de NeoGuitar (Inno Setup 6).
; Compilar: iscc /DAppVersion=0.1.0 installer\neoguitar.iss   (genera dist\NeoGuitar-<versión>-setup.exe)
; Requiere haber compilado antes: cargo build --release

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
LicenseFile=..\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Por defecto se instala solo para el usuario (sin permisos de administrador); el asistente permite elegir para todos.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes
UninstallDisplayIcon={app}\{#AppExe}

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "assoc"; Description: "Asociar los archivos Guitar Pro (.gp3, .gp4, .gp5, .gpx, .gp) con NeoGuitar"; GroupDescription: "Asociaciones de archivo:"
Name: "desktopicon"; Description: "Crear un icono en el escritorio"; GroupDescription: "Accesos directos:"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\soundfonts\*"; DestDir: "{app}\soundfonts"; Flags: ignoreversion recursesubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\NeoGuitar"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\NeoGuitar"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; HKA = HKCU si se instala solo para el usuario, HKLM si es para todos.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "Archivo Guitar Pro"; Flags: uninsdeletekey; Tasks: assoc
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
Filename: "{app}\{#AppExe}"; Description: "Abrir NeoGuitar"; Flags: nowait postinstall skipifsilent
