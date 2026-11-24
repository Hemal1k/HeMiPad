; HeMiPad installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=1.1.0 /DAppExe=<HeMiPad.exe> /DOutputDir=<folder>
; For the current user (no administrator) or for all users – the installer asks. Uninstalling removes the File Explorer item of the app as well.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef AppExe
  #define AppExe "..\build\release\HeMiPad.exe"
#endif
#ifndef AppExeArm
  #define AppExeArm ""
#endif
#ifndef OutputDir
  #define OutputDir "..\build\dist"
#endif

[Setup]
AppId=HeMiPad
AppName=HeMiPad
AppVersion={#AppVersion}
AppVerName=HeMiPad {#AppVersion}
AppPublisher=Richard Hemala
AppPublisherURL=https://github.com/Hemal1k/HeMiPad
AppSupportURL=https://github.com/Hemal1k/HeMiPad/issues
AppUpdatesURL=https://github.com/Hemal1k/HeMiPad/releases
DefaultDirName={autopf}\HeMi\HeMiPad
UsePreviousAppDir=no
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; the first page asks: for me only (default, no administrator) or for all users (Program Files, asks for administrator)
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible or arm64
ArchitecturesInstallIn64BitMode=x64compatible or arm64
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=HeMiPad-Setup
SetupIconFile=..\assets\HeMiPad.ico
UninstallDisplayIcon={app}\HeMiPad.exe
UninstallDisplayName=HeMiPad
Compression=lzma2/max
SolidCompression=yes
; the look of Windows 11, light or dark as Windows, and as big as the first question of the installer (it was
; smaller and old-looking next to it, 2026-11-19)
WizardStyle=modern dynamic windows11
WizardSizePercent=120
CloseApplications=force
LicenseFile=..\LICENSE
; the language of Windows without asking (English when Windows is neither in English nor in Czech)
ShowLanguageDialog=auto

[Languages]
; English, and Czech when Windows is in Czech
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "cs"; MessagesFile: "compiler:Languages\Czech.isl"

[CustomMessages]
en.DeleteSettings=Delete the settings of HeMiPad too?%n%nKeep them to use them again when you install HeMiPad later.
cs.DeleteSettings=Smazat i nastavení HeMiPadu?%n%nKdyž ho necháš, použije se znovu při další instalaci HeMiPadu.

[Files]
; x64, and the ARM64 program on Windows on ARM when the installer has it
#if AppExeArm != ""
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion; Check: not IsArm64
Source: "{#AppExeArm}"; DestDir: "{app}"; DestName: "HeMiPad.exe"; Flags: ignoreversion; Check: IsArm64
#else
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
#endif

[Tasks]
; offered on its own page, off by default
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Icons]
Name: "{autoprograms}\HeMiPad"; Filename: "{app}\HeMiPad.exe"
Name: "{autodesktop}\HeMiPad"; Filename: "{app}\HeMiPad.exe"; Tasks: desktopicon

[Run]
; the File Explorer item points to this installation (also after a move from the old folder)
Filename: "{app}\HeMiPad.exe"; Parameters: "--repair"; Flags: runasoriginaluser waituntilterminated runhidden
Filename: "{app}\HeMiPad.exe"; Description: "{cm:LaunchProgram,HeMiPad}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Registry]
; the File Explorer item the app writes when it is switched on: removed with the app
Root: HKCU; Subkey: "Software\Classes\*\shell\HeMiPad"; Flags: uninsdeletekey dontcreatekey
; what Windows itself keeps about the app (its name in menus, "Open with")
Root: HKCU; Subkey: "Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"; ValueType: none; ValueName: "{app}\HeMiPad.exe.FriendlyAppName"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"; ValueType: none; ValueName: "{app}\HeMiPad.exe.ApplicationCompany"; Flags: uninsdeletevalue dontcreatekey
Root: HKCU; Subkey: "Software\Classes\Applications\HeMiPad.exe"; Flags: uninsdeletekey dontcreatekey
Root: HKCU; Subkey: "Software\HeMi"; Flags: uninsdeletekeyifempty dontcreatekey

[Code]
// Since 2026-11 the HeMi apps go into one folder HeMi, as their settings do (%AppData%\HeMi\<app>): {autopf}\HeMi\HeMiPad.
// An installation in the old folder {autopf}\HeMiPad moves: the app running from there ends, the new version goes into the
// new folder ("--repair" points the File Explorer item to it) and the old folder is deleted.
var
  OldFolder: String;

function FindOldFolder: String;
begin
  if RegQueryStringValue(HKA, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\HeMiPad_is1', 'InstallLocation', Result) then
  begin
    Result := RemoveBackslashUnlessRoot(Result);
    if (CompareText(Result, ExpandConstant('{app}')) = 0) or (CompareText(ExtractFileName(Result), 'HeMiPad') <> 0)
      or not FileExists(Result + '\HeMiPad.exe') then
      Result := '';
  end
  else
    Result := '';
end;

procedure StopOld;
var
  Code: Integer;
begin
  OldFolder := FindOldFolder;
  if OldFolder = '' then Exit;
  Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -Command "Get-Process | Where-Object { $_.Path -like ''' + OldFolder + '\*'' } | Stop-Process -Force"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

procedure DeleteOld;
begin
  if OldFolder <> '' then DelTree(OldFolder, True, True, True);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  StopOld;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then DeleteOld;
end;

// Uninstalling: the settings of the app (%AppData%\HeMi\HeMiPad) are deleted or kept, as the user chooses (a quiet
// uninstall keeps them). %AppData%\HeMi goes too when no other HeMi app has anything there.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dir: String;
begin
  if CurUninstallStep <> usPostUninstall then Exit;
  Dir := ExpandConstant('{userappdata}\HeMi\HeMiPad');
  if not DirExists(Dir) then Exit;
  if UninstallSilent then Exit;
  if MsgBox(CustomMessage('DeleteSettings'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
  begin
    DelTree(Dir, True, True, True);
    RemoveDir(ExpandConstant('{userappdata}\HeMi'));
  end;
end;
