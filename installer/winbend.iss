; WinBend installer (Inno Setup 6). Built by dist\package.ps1 and by CI.
;   iscc /DAppVersion=0.1.1 /DSourceDir=..\target\release installer\winbend.iss
; Installs per user (no admin prompt) into %LOCALAPPDATA%\Programs\WinBend, adds a Start menu
; entry, optional desktop shortcut and optional run-at-startup, and offers to launch at the end.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif

[Setup]
AppId={{7C1B2E3A-6D0B-4A3E-9F0D-WINBEND000001}
AppName=WinBend
AppVersion={#AppVersion}
AppVerName=WinBend {#AppVersion}
AppPublisher=MineGL
AppPublisherURL=https://winbend.me
AppSupportURL=https://github.com/MineGL/WinBend/issues
AppUpdatesURL=https://github.com/MineGL/WinBend/releases
DefaultDirName={userpf}\WinBend
DefaultGroupName=WinBend
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=WinBend-Setup-{#AppVersion}
SetupIconFile=winbend.ico
UninstallDisplayIcon={app}\winbend.exe
UninstallDisplayName=WinBend
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
LicenseFile=..\LICENSE
CloseApplications=yes
RestartApplications=no
VersionInfoVersion={#AppVersion}.0
VersionInfoDescription=WinBend Setup
VersionInfoCopyright=Copyright (c) 2026 MineGL and WinBend contributors

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Start WinBend when I sign in (tray icon)"; GroupDescription: "Startup:"

[Files]
Source: "{#SourceDir}\winbend.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{group}\WinBend"; Filename: "{app}\winbend.exe"; Comment: "Your desktop folds like a lid closing"
Name: "{group}\WinBend website"; Filename: "https://winbend.me"
Name: "{autodesktop}\WinBend"; Filename: "{app}\winbend.exe"; Tasks: desktopicon

[Registry]
; Same key and value WinBend's own "Run at startup" switch uses, so the app's toggle stays in sync.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "WinBend"; ValueData: """{app}\winbend.exe"""; Tasks: startup; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "WinBend"; Flags: uninsdeletevalue dontcreatekey

[Run]
Filename: "{app}\winbend.exe"; Description: "Launch WinBend now"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "taskkill.exe"; Parameters: "/IM winbend.exe /F"; Flags: runhidden; RunOnceId: "KillWinBend"

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\WinBend\WebView2"

[Code]
// WinBend's tray window is message-only, so Restart Manager cannot close it; stop it ourselves
// before files are replaced.
procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
begin
  if CurStep = ssInstall then
    Exec('taskkill.exe', '/IM winbend.exe /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  if SuppressibleMsgBox('If you turned on "WinBend handles the lid" or "Skip the sign-in screen after sleep" ' +
            'in Settings > Windows, turn them off before uninstalling so Windows goes back to its ' +
            'normal lid and sign-in behaviour.' + #13#10#13#10 + 'Continue with the uninstall?',
            mbConfirmation, MB_YESNO, IDYES) = IDNO then
    Result := False;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    if SuppressibleMsgBox('Also delete your WinBend settings (%APPDATA%\WinBend)?', mbConfirmation, MB_YESNO, IDNO) = IDYES then
      DelTree(ExpandConstant('{userappdata}\WinBend'), True, True, True);
end;
