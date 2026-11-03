#define AppVersion "0.1.3"
[Setup]
AppId={{BCF882D6-2071-4E38-A957-54E5805439F7}
AppName=DoubaoPTT
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\DoubaoPTT
DefaultGroupName=DoubaoPTT
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist
OutputBaseFilename=DoubaoPTT-Setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\doubao-ptt.exe
[Files]
Source: "..\dist\doubao-ptt.exe"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{group}\DoubaoPTT"; Filename: "{app}\doubao-ptt.exe"; Parameters: "serve --open"
[Run]
Filename: "{app}\doubao-ptt.exe"; Parameters: "serve --open"; Description: "Open DoubaoPTT setup"; Flags: nowait postinstall skipifsilent
[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\DoubaoPTT\DoubaoPTT\data"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "DoubaoPTT"; Flags: uninsdeletevalue
