; HeMiPad installer (Inno Setup 6), built by release.ps1:  ISCC /DAppVersion=1.1.0 /DAppExe=<HeMiPad.exe> /DOutputDir=<folder>
; For the current user (no administrator) or for all users - the installer asks. Uninstalling removes the File Explorer item of the app as well.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef AppExe
  #define AppExe "..\build\release\HeMiPad.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\build\dist"
#endif

[Setup]
AppId=HeMiPad
AppName=HeMiPad
AppVersion={#AppVersion}
AppVerName=HeMiPad Beta {#AppVersion}
AppPublisher=Richard Hemala
AppPublisherURL=https://github.com/Hemal1k/HeMiPad
AppSupportURL=https://github.com/Hemal1k/HeMiPad/issues
AppUpdatesURL=https://github.com/Hemal1k/HeMiPad/releases
DefaultDirName={autopf}\HeMiPad
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
; the first page asks: for me only (default, no administrator) or for all users (Program Files, asks for administrator)
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputDir}
OutputBaseFilename=HeMiPad-Setup
SetupIconFile=..\assets\HeMiPad.ico
UninstallDisplayIcon={app}\HeMiPad.exe
UninstallDisplayName=HeMiPad
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
LicenseFile=..\LICENSE

[Languages]
; English only
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Tasks]
; offered on its own page, off by default
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Icons]
Name: "{autoprograms}\HeMiPad"; Filename: "{app}\HeMiPad.exe"
Name: "{autodesktop}\HeMiPad"; Filename: "{app}\HeMiPad.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\HeMiPad.exe"; Description: "{cm:LaunchProgram,HeMiPad}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Registry]
; the File Explorer item the app writes when it is switched on: removed with the app
Root: HKCU; Subkey: "Software\Classes\*\shell\HeMiPad"; Flags: uninsdeletekey dontcreatekey
