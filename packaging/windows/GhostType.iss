; The installer of GhostType for the current user, from a folder that stage.ps1 filled.
; ISCC /DAppVersion=0.1.0 /DFlavor=vulkan /DStagingDir=C:\path\to\app /OC:\path\to\output GhostType.iss

#ifndef AppVersion
  #error Give the version with /DAppVersion=x.y.z
#endif
#ifndef Flavor
  #error Give the backends in the file name with /DFlavor=vulkan or /DFlavor=cuda
#endif
#ifndef StagingDir
  #error Give the folder that stage.ps1 filled with /DStagingDir=path
#endif

[Setup]
; All flavors use the same AppId, so one flavor replaces the other.
AppId={{8BAE0CC1-DFF5-4021-8D1C-3A93AB900E66}
AppName=GhostType
AppVersion={#AppVersion}
AppPublisher=GhostType contributors
AppPublisherURL=https://github.com/IsraPerez98/GhostType
AppSupportURL=https://github.com/IsraPerez98/GhostType/issues
AppUpdatesURL=https://github.com/IsraPerez98/GhostType/releases
; GhostType holds this mutex while it runs. Setup and the uninstaller ask the user to quit it first.
AppMutex=Local\GhostType.SingleInstance
PrivilegesRequired=lowest
DefaultDirName={autopf}\GhostType
; The autostart value of GhostType points to the program, so each update must use the same folder.
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputBaseFilename=GhostType-{#AppVersion}-windows-x64-{#Flavor}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=GhostType
UninstallDisplayIcon={app}\GhostType.exe

[InstallDelete]
; Removes the DLLs of the other flavor, so llama.cpp does not load a backend of an older install.
Type: files; Name: "{app}\*.dll"

[Files]
Source: "{#StagingDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\GhostType"; Filename: "{app}\GhostType.exe"

[Run]
Filename: "{app}\GhostType.exe"; Description: "Start GhostType"; Flags: nowait postinstall skipifsilent

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  StartupApprovedKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';
  AutostartValue = 'GhostType';

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, RunKey, AutostartValue);
    RegDeleteValue(HKEY_CURRENT_USER, StartupApprovedKey, AutostartValue);
  end;
end;
