; Per-user installer for Modlist Launcher. Needs no administrator rights.
; Built by scripts/package.ps1, which passes AppVersion and SourceDir.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\dist\staging"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{6B1E2B53-7C1E-4E0F-9A35-2C7B8F0D4A61}
AppName=Modlist Launcher
AppVersion={#AppVersion}
AppPublisher=Modlist Launcher contributors
AppPublisherURL=https://github.com/jmars319/modlist-launcher
AppSupportURL=https://github.com/jmars319/modlist-launcher/issues
DefaultDirName={autopf}\Modlist Launcher
DefaultGroupName=Modlist Launcher
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile={#SourceDir}\LICENSE.txt
OutputDir={#OutputDir}
OutputBaseFilename=ModlistLauncher-{#AppVersion}-setup
SetupIconFile=..\assets\app.ico
UninstallDisplayIcon={app}\modlist-launcher.exe
UninstallDisplayName=Modlist Launcher
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\modlist-launcher.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; An explicit IconFilename, so Windows refreshes the icon when the exe changes.
Name: "{autoprograms}\Modlist Launcher"; Filename: "{app}\modlist-launcher.exe"; IconFilename: "{app}\modlist-launcher.exe"
Name: "{autodesktop}\Modlist Launcher"; Filename: "{app}\modlist-launcher.exe"; IconFilename: "{app}\modlist-launcher.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\modlist-launcher.exe"; Description: "Open Modlist Launcher"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Puts back each game's previous Steam launch options, so Steam's Play button
; does not point at the removed launcher. Silent when there is nothing to do;
; asks to exit Steam first if it is running.
Filename: "{app}\modlist-launcher.exe"; Parameters: "--restore-steam"; RunOnceId: "RestoreSteamLaunchOptions"; Flags: waituntilterminated

; The launcher's own settings in %LOCALAPPDATA%\Modlist Launcher are kept on
; uninstall, and nothing inside any Mod Organizer installation was ever written.
