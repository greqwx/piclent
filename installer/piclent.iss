; piclent installer.
; Compile: ISCC.exe /DAppVersion=1.2.3 /DAppVersionNumeric=1.2.3 piclent.iss
; Expects payload\piclent-amd64.exe and payload\piclent-arm64.exe next to this file.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif

#ifndef AppVersionNumeric
  #define AppVersionNumeric "0.0.0.0"
#endif

[Setup]
AppId={{6C1D5F1B-3B0F-4C8E-9E4A-2F0C0E1A7B31}
AppName=piclent
AppVersion={#AppVersion}
AppVerName=piclent {#AppVersion}
AppPublisher=Greg
AppCopyright=Copyright (c) 2026 Greg and Contributors
DefaultDirName={autopf}\piclent
DefaultGroupName=piclent
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=Output
OutputBaseFilename=piclent-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\piclent.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
VersionInfoVersion={#AppVersionNumeric}
VersionInfoProductVersion={#AppVersionNumeric}
VersionInfoCompany=Greg
VersionInfoDescription=piclent image viewer
VersionInfoProductName=piclent
VersionInfoCopyright=Copyright (c) 2026 Greg and Contributors

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
TaskGroup=Optional
TaskPath=Add piclent to PATH (open images from a terminal)
TaskAssoc=Add piclent to the "Open with" menu for .png, .jpg, .jpeg, .gif
TaskDefault=Open Windows settings to choose piclent as the default viewer
PathWriteFailed=Could not write to the PATH variable. You can add this folder manually:
russian.TaskGroup=Дополнительно
russian.TaskPath=Добавить piclent в PATH (открывать картинки из терминала)
russian.TaskAssoc=Пункт piclent в «Открыть с помощью» для .png, .jpg, .jpeg, .gif
russian.TaskDefault=Открыть настройки, чтобы выбрать piclent по умолчанию
russian.PathWriteFailed=Не удалось записать PATH. Добавьте папку вручную:

[Tasks]
Name: "path"; Description: "{cm:TaskPath}"; GroupDescription: "{cm:TaskGroup}"
Name: "assoc"; Description: "{cm:TaskAssoc}"; GroupDescription: "{cm:TaskGroup}"
Name: "defaultapp"; Description: "{cm:TaskDefault}"; GroupDescription: "{cm:TaskGroup}"; Flags: unchecked
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "payload\piclent-amd64.exe"; DestDir: "{app}"; DestName: "piclent.exe"; Flags: ignoreversion; Check: not IsArmMachine
Source: "payload\piclent-arm64.exe"; DestDir: "{app}"; DestName: "piclent.exe"; Flags: ignoreversion; Check: IsArmMachine
Source: "..\license.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\piclent"; Filename: "{app}\piclent.exe"
Name: "{autodesktop}\piclent"; Filename: "{app}\piclent.exe"; Tasks: desktopicon

[Registry]
; ProgID for the viewer, lives in HKCR (per-machine when installing for all users)
Root: HKA; Subkey: "Software\Classes\piclent.image"; ValueType: string; ValueName: ""; ValueData: "Image file"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\piclent.image\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\piclent.exe,0"
Root: HKA; Subkey: "Software\Classes\piclent.image\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\piclent.exe"" ""%1"""; Flags: uninsdeletekey

; "Open with" entries. Existing file associations stay untouched.
Root: HKA; Subkey: "Software\Classes\.png\OpenWithProgids"; ValueType: none; ValueName: "piclent.image"; Tasks: assoc; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpg\OpenWithProgids"; ValueType: none; ValueName: "piclent.image"; Tasks: assoc; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.jpeg\OpenWithProgids"; ValueType: none; ValueName: "piclent.image"; Tasks: assoc; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gif\OpenWithProgids"; ValueType: none; ValueName: "piclent.image"; Tasks: assoc; Flags: uninsdeletevalue

; Makes piclent show up on the "Default apps" page in Settings.
Root: HKA; Subkey: "Software\piclent\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "piclent"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\piclent\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Tiny image viewer"
Root: HKA; Subkey: "Software\piclent\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\piclent.exe,0"
Root: HKA; Subkey: "Software\piclent\Capabilities\FileAssociations"; ValueType: string; ValueName: ".png"; ValueData: "piclent.image"
Root: HKA; Subkey: "Software\piclent\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpg"; ValueData: "piclent.image"
Root: HKA; Subkey: "Software\piclent\Capabilities\FileAssociations"; ValueType: string; ValueName: ".jpeg"; ValueData: "piclent.image"
Root: HKA; Subkey: "Software\piclent\Capabilities\FileAssociations"; ValueType: string; ValueName: ".gif"; ValueData: "piclent.image"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "piclent"; ValueData: "Software\piclent\Capabilities"; Flags: uninsdeletevalue

; Lets Win+R and explorer resolve "piclent" without a full path.
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\piclent.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\piclent.exe"; Flags: uninsdeletekey

[Run]
Filename: "ms-settings:defaultapps?registeredAppUser=piclent"; Tasks: defaultapp; Flags: shellexec nowait skipifsilent
Filename: "{app}\piclent.exe"; Description: "{cm:LaunchProgram,piclent}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WM_SETTINGCHANGE = $001A;
  InstallKey = 'Software\piclent\Install';

function IsArmMachine: Boolean;
begin
  Result := IsArm64;
end;

function EnvRoot: Integer;
begin
  if IsAdminInstallMode then
    Result := HKLM
  else
    Result := HKCU;
end;

function EnvSubKey: String;
begin
  if IsAdminInstallMode then
    Result := 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
  else
    Result := 'Environment';
end;

function MarkerRoot: Integer;
begin
  if IsAdminInstallMode then
    Result := HKLM
  else
    Result := HKCU;
end;

function TrimDir(const S: String): String;
begin
  Result := Lowercase(Trim(S));
  if Result <> '' then
    while (Length(Result) > 1) and (Result[Length(Result)] = '\') do
      Delete(Result, Length(Result), 1);
end;

function PathHasDir(const PathValue, Dir: String): Boolean;
var
  Rest, Item, Needle: String;
  P: Integer;
begin
  Result := False;
  Needle := TrimDir(Dir);
  Rest := PathValue;
  while True do
  begin
    P := Pos(';', Rest);
    if P > 0 then
    begin
      Item := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end
    else
    begin
      Item := Rest;
      Rest := '';
    end;
    if TrimDir(Item) = Needle then
    begin
      Result := True;
      Exit;
    end;
    if P = 0 then
      Break;
  end;
end;

function PathWithoutDir(const PathValue, Dir: String): String;
var
  Rest, Item, Needle: String;
  P: Integer;
begin
  Result := '';
  Needle := TrimDir(Dir);
  Rest := PathValue;
  while True do
  begin
    P := Pos(';', Rest);
    if P > 0 then
    begin
      Item := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end
    else
    begin
      Item := Rest;
      Rest := '';
    end;
    if Trim(Item) <> '' then
      if TrimDir(Item) <> Needle then
      begin
        if Result <> '' then
          Result := Result + ';';
        Result := Result + Trim(Item);
      end;
    if P = 0 then
      Break;
  end;
end;

procedure AddToPath;
var
  Current, AppDir: String;
begin
  AppDir := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  if not RegQueryStringValue(EnvRoot, EnvSubKey, 'Path', Current) then
    Current := '';
  if PathHasDir(Current, AppDir) then
  begin
    RegWriteDWordValue(MarkerRoot, InstallKey, 'PathAdded', 0);
    Exit;
  end;

  if (Current <> '') and (Current[Length(Current)] <> ';') then
    Current := Current + ';';

  if not RegWriteExpandStringValue(EnvRoot, EnvSubKey, 'Path', Current + AppDir) then
  begin
    MsgBox(ExpandConstant('{cm:PathWriteFailed}') + #13#10 + AppDir, mbError, MB_OK);
    Exit;
  end;

  RegWriteDWordValue(MarkerRoot, InstallKey, 'PathAdded', 1);
  SendBroadcastMessage(WM_SETTINGCHANGE, 0, 0);
end;

procedure RemoveFromPath;
var
  Current, AppDir, Updated: String;
  Added: Cardinal;
begin
  if not RegQueryDWordValue(MarkerRoot, InstallKey, 'PathAdded', Added) then
    Exit;
  if Added <> 1 then
    Exit;
  if not RegQueryStringValue(EnvRoot, EnvSubKey, 'Path', Current) then
    Exit;

  AppDir := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  Updated := PathWithoutDir(Current, AppDir);
  if Updated = Current then
    Exit;

  RegWriteExpandStringValue(EnvRoot, EnvSubKey, 'Path', Updated);
  SendBroadcastMessage(WM_SETTINGCHANGE, 0, 0);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    if WizardIsTaskSelected('path') then
      AddToPath;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath;
end;
