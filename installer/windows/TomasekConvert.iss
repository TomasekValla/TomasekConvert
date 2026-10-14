; Inno Setup – instalátor pro Windows (x64 i ARM64).
; Sestavení: ISCC /DAppVersion=2.2.0 /DExePath=cesta\k\TomasekConvert.exe /DArch=x64 TomasekConvert.iss
#define AppName "TomasekConvert"
#define AppUrl "https://github.com/TomasekValla/TomasekConvert"
#ifndef AppVersion
  #define AppVersion "2.2.0"
#endif
#ifndef ExePath
  #define ExePath "..\..\target\release\TomasekConvert.exe"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
; Cesta k vc_redist – CI ho stáhne vedle .iss; při lokálním sestavení nastavit ručně nebo vynechat
#ifndef VcRedistPath
  #define VcRedistPath "vc_redist.{#Arch}.exe"
#endif

[Setup]
AppId={{B6D7A0F2-6E55-4F61-9C0B-3C1E1B1D9A11}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayIcon={app}\TomasekConvert.exe
SetupIconFile=..\..\assets\icon.ico
OutputBaseFilename=TomasekConvert-Setup-{#Arch}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible arm64
ArchitecturesInstallIn64BitMode=x64compatible arm64
; bez práv správce se instaluje jen pro aktuálního uživatele, instalátor se na to při spuštění zeptá
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "czech"; MessagesFile: "compiler:Languages\Czech.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Automatically run on PC startup"; GroupDescription: "Startup:"; Flags: unchecked
Name: "autostart\tray"; Description: "Start quietly in the background (system tray) without opening a window"; GroupDescription: "Startup:"; Flags: unchecked
Name: "openwith"; Description: "Add to the "Open with" list"; GroupDescription: "Windows Explorer integration:"
Name: "contextmenu"; Description: "Add "Convert with TomasekConvert" to the right-click menu of files and folders"; GroupDescription: "Windows Explorer integration:"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "TomasekConvert.exe"; Flags: ignoreversion
; Visual C++ Redistributable – přibalen jen pokud existuje vedle .iss (CI ho stáhne)
Source: "{#VcRedistPath}"; DestDir: "{tmp}"; Flags: deleteafterinstall; Check: VcRedistFileExists

[Icons]
; zástupce v nabídce Start (hledání ve Windows ukáže "TomasekConvert" s logem)
Name: "{group}\{#AppName}"; Filename: "{app}\TomasekConvert.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\TomasekConvert.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TomasekConvert"; ValueData: """{app}\TomasekConvert.exe"""; Tasks: autostart; Check: not WizardIsTaskSelected('autostart\tray'); Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "TomasekConvert"; ValueData: """{app}\TomasekConvert.exe"" --tray"; Tasks: autostart\tray; Flags: uninsdeletevalue

Root: HKCU; Subkey: "Software\Classes\Applications\TomasekConvert.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "TomasekConvert"; Tasks: openwith; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\TomasekConvert.exe\shell\open\command"; ValueType: string; ValueData: """{app}\TomasekConvert.exe"" ""%1"""; Tasks: openwith

Root: HKCU; Subkey: "Software\Classes\*\shell\TomasekConvert"; ValueType: string; ValueData: "Convert with TomasekConvert"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\*\shell\TomasekConvert"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\TomasekConvert.exe"",0"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\*\shell\TomasekConvert\command"; ValueType: string; ValueData: """{app}\TomasekConvert.exe"" ""%1"""; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\TomasekConvert"; ValueType: string; ValueData: "Convert with TomasekConvert"; Tasks: contextmenu; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\TomasekConvert"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\TomasekConvert.exe"",0"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\TomasekConvert\command"; ValueType: string; ValueData: """{app}\TomasekConvert.exe"" ""%1"""; Tasks: contextmenu

[Run]
; Visual C++ Redistributable – tiché spuštění jen pokud je na stroji potřeba (chybí VCRUNTIME140.dll)
Filename: "{tmp}\{#VcRedistPath}"; Parameters: "/install /quiet /norestart"; Flags: waituntilterminated; Check: VcRedistNeeded
Filename: "{app}\TomasekConvert.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; samoaktualizace: appka spustí tichý instalátor s /RELAUNCH=1 (okno) nebo /RELAUNCH=tray (jen na pozadí) a po instalaci se sama znovu spustí
Filename: "{app}\TomasekConvert.exe"; Parameters: "--after-update"; Flags: nowait runasoriginaluser; Check: RelaunchVisible
Filename: "{app}\TomasekConvert.exe"; Parameters: "--after-update --tray"; Flags: nowait runasoriginaluser; Check: RelaunchTray

[Code]
function RelaunchVisible: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = '1';
end;

function RelaunchTray: Boolean;
begin
  Result := ExpandConstant('{param:RELAUNCH|0}') = 'tray';
end;

{ Existuje soubor vc_redist vedle skriptu? (CI ho stáhne, lokálně nemusí být) }
function VcRedistFileExists: Boolean;
begin
  Result := FileExists(ExpandConstant('{src}\{#VcRedistPath}'));
end;

{ Chybí VCRUNTIME140.dll? Pak redist potřebujeme nainstalovat. }
function VcRedistNeeded: Boolean;
begin
  Result := VcRedistFileExists and
            not FileExists(ExpandConstant('{sys}\VCRUNTIME140.dll'));
end;
