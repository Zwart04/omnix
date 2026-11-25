; Omnix installer for Windows x64 (Inno Setup 6).
; Per-user install: no admin prompt, WebView2 bootstrapper runs only when the
; runtime is missing, and the app restarts after a silent auto-update.
#define MyAppName "Omnix"
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppExeName "omnix.exe"

[Setup]
AppId={{6C2E7A34-9A1B-4E55-9F3D-2B7C8E1D4A60}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=Zwart04
AppPublisherURL=https://github.com/Zwart04/omnix
AppSupportURL=https://github.com/Zwart04/omnix/issues
AppUpdatesURL=https://github.com/Zwart04/omnix/releases
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=build
OutputBaseFilename=Omnix-{#MyAppVersion}-Windows-x64-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
MinVersion=10.0
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#MyAppExeName}
LicenseFile=..\..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "build\omnix.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "MicrosoftEdgeWebView2Setup.exe"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall

[Code]
const
  WebView2Client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function WebView2Missing: Boolean;
var
  Version: String;
begin
  Result :=
    not RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version) and
    not RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version) and
    not RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\' + WebView2Client, 'pv', Version);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Result := '';
  if WebView2Missing then
  begin
    ExtractTemporaryFile('MicrosoftEdgeWebView2Setup.exe');
    if (not Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebView2Setup.exe'),
                 '/silent /install', '', SW_SHOW, ewWaitUntilTerminated, Code))
       or (Code <> 0) then
      Result := 'Omnix needs the Microsoft Edge WebView2 runtime. ' +
                'Install it from https://go.microsoft.com/fwlink/p/?LinkId=2124703, ' +
                'then run this setup again.';
  end;
end;
