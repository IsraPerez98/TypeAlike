; The installer of TypeAlike for the current user, from a folder that stage.ps1 filled.
; ISCC /DAppVersion=0.1.0 /DFlavor=vulkan /DStagingDir=C:\path\to\app /OC:\path\to\output TypeAlike.iss

#ifndef AppVersion
  #error Give the version with /DAppVersion=x.y.z
#endif
#ifndef Flavor
  #error Give the backends in the file name with /DFlavor=vulkan or /DFlavor=cuda
#endif
#ifndef StagingDir
  #error Give the folder that stage.ps1 filled with /DStagingDir=path
#endif

#define AppMutexName "Local\TypeAlike.SingleInstance"

[Setup]
; All flavors use the same AppId, so one flavor replaces the other.
AppId={{353C0522-974C-4AC9-9B51-60816CE47E06}
AppName=TypeAlike
AppVersion={#AppVersion}
AppPublisher=TypeAlike contributors
AppPublisherURL=https://github.com/IsraPerez98/TypeAlike
AppSupportURL=https://github.com/IsraPerez98/TypeAlike/issues
AppUpdatesURL=https://github.com/IsraPerez98/TypeAlike/releases
; TypeAlike holds this mutex while it runs. Setup and the uninstaller ask the user to quit it first.
AppMutex={#AppMutexName}
PrivilegesRequired=lowest
DefaultDirName={autopf}\TypeAlike
; The autostart value of TypeAlike points to the program, so each update must use the same folder.
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputBaseFilename=TypeAlike-{#AppVersion}-windows-x64-{#Flavor}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=TypeAlike
UninstallDisplayIcon={app}\TypeAlike.exe
SetupIconFile=..\..\crates\typealike\TypeAlike.ico

[InstallDelete]
; Removes the DLLs of the other flavor, so llama.cpp does not load a backend of an older install.
Type: files; Name: "{app}\*.dll"

[Files]
Source: "{#StagingDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\TypeAlike"; Filename: "{app}\TypeAlike.exe"

[Run]
Filename: "{app}\TypeAlike.exe"; Description: "Start TypeAlike"; Flags: nowait postinstall skipifsilent
; TypeAlike runs this setup without a window to update itself, and quits. The new version then starts.
Filename: "{app}\TypeAlike.exe"; Flags: nowait skipifnotsilent

[Code]
const
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  StartupApprovedKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run';
  AutostartValue = 'TypeAlike';
  QuitWaitMilliseconds = 30000;
  QuitCheckMilliseconds = 100;

{ An update starts this setup silently while TypeAlike quits, and Setup checks AppMutex after this function. }
function InitializeSetup: Boolean;
var
  Waited: Integer;
begin
  Waited := 0;
  while WizardSilent and CheckForMutexes('{#AppMutexName}') and (Waited < QuitWaitMilliseconds) do
  begin
    Sleep(QuitCheckMilliseconds);
    Waited := Waited + QuitCheckMilliseconds;
  end;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, RunKey, AutostartValue);
    RegDeleteValue(HKEY_CURRENT_USER, StartupApprovedKey, AutostartValue);
  end;
end;
