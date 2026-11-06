#ifndef AppVersion
#define AppVersion "0.2.6"
#endif
#include "..\artifacts\vendor\vb-cable-45\vbcable.generated.iss"
[Setup]
AppId={{BCF882D6-2071-4E38-A957-54E5805439F7}
AppName=豆包远程
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\DoubaoRemote
DefaultGroupName=豆包远程
UsePreviousAppDir=yes
DisableDirPage=auto
UsePreviousGroup=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist
OutputBaseFilename=DoubaoRemote-Setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
; ExtractVerifiedCable opens the vendor's original ZIP. The enhanced modes only support 7z.
ArchiveExtraction=full
LicenseFile=..\artifacts\vendor\vb-cable-45\windows-license.txt
SetupIconFile=..\assets\icons\doubao-remote.ico
UninstallDisplayIcon={app}\doubao-remote.exe
[Files]
; Preserve the vendor's complete original archive, including its original EULA.
Source: "..\artifacts\vendor\vb-cable-45\VBCABLE_Driver_Pack45.zip"; Flags: dontcopy
Source: "..\artifacts\vendor\vb-cable-45\vbcable-files.sha256"; Flags: dontcopy
Source: "..\dist\doubao-remote.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\artifacts\vendor\vb-cable-45\windows-license.txt"; DestDir: "{app}"; DestName: "THIRD-PARTY-LICENSES.txt"; Flags: ignoreversion
[Icons]
Name: "{group}\豆包远程"; Filename: "{app}\doubao-remote.exe"; Parameters: "serve --open"
Name: "{userdesktop}\豆包远程"; Filename: "{app}\doubao-remote.exe"; Parameters: "serve --open"; WorkingDir: "{app}"
[Run]
Filename: "{app}\doubao-remote.exe"; Parameters: "serve --open"; Description: "打开豆包远程设置"; Flags: nowait postinstall skipifsilent; Check: CanLaunchAfterInstall
[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\DoubaoRemote\DoubaoRemote\data"
; Compatibility: remove legacy per-user data only on explicit uninstall.
Type: filesandordirs; Name: "{localappdata}\DoubaoPTT\DoubaoPTT\data"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "DoubaoRemote"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "DoubaoPTT"; Flags: uninsdeletevalue

[Code]
var
  AudioPage: TOutputMsgWizardPage;
  AudioNeedsRestart: Boolean;
  VendorLocks: array of TFileStream;
  UpgradePrepared, HadSettings, HadNewExe, HadNewRun, HadOldRun: Boolean;
  UpgradeData, NewRunValue, OldRunValue, PreparedAppPath, BackupDir: String;
  HadDesktopLink, HadMenuLink: Boolean;

{ Legacy names below are for covering existing installations, never display names. }
function HasData(const Directory: String): Boolean;
begin
  Result := FileExists(Directory + '\settings.json') or FileExists(Directory + '\devices.json') or
    FileExists(Directory + '\ca.pem') or FileExists(Directory + '\ca-key.dpapi') or
    FileExists(Directory + '\mock-ca-key.pem');
end;

function PrepareUpgrade: String;
var Locator, Services, Processes, Process: Variant; I, Number: Integer; Path, OldData, BackupBase: String;
begin
  Result := '';
  if UpgradePrepared and (PreparedAppPath = ExpandConstant('{app}')) then Exit;
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Services := Locator.ConnectServer('', 'root\CIMV2');
    Processes := Services.ExecQuery('SELECT ExecutablePath FROM Win32_Process WHERE Name = ''doubao-ptt.exe'' OR Name = ''doubao-remote.exe''');
    for I := 0 to Processes.Count - 1 do begin
      Process := Processes.ItemIndex(I);
      if not VarIsNull(Process.ExecutablePath) then begin
        Path := Process.ExecutablePath;
        if CompareText(ExtractFileDir(Path), ExpandConstant('{app}')) = 0 then begin
          Result := '请先从托盘退出正在运行的豆包远程，再重新安装。'; Exit;
        end;
      end;
    end;
    UpgradeData := ExpandConstant('{localappdata}\DoubaoRemote\DoubaoRemote\data');
    OldData := ExpandConstant('{localappdata}\DoubaoPTT\DoubaoPTT\data');
    if not HasData(UpgradeData) and HasData(OldData) then UpgradeData := OldData;
    BackupBase := ExpandConstant('{localappdata}\DoubaoRemote\upgrade-backup\') + GetDateTimeString('yyyymmdd-hhnnss', '-', ':');
    BackupDir := BackupBase;
    Number := 0;
    while DirExists(BackupDir) do begin Number := Number + 1; BackupDir := BackupBase + '-' + IntToStr(Number); end;
    if not ForceDirectories(BackupDir) then RaiseException('无法创建升级备份目录。');
    HadSettings := FileExists(UpgradeData + '\settings.json');
    if HadSettings and not FileCopy(UpgradeData + '\settings.json', BackupDir + '\settings-before.json', False) then
      RaiseException('无法备份原有设置，已停止安装。');
    HadNewExe := FileExists(ExpandConstant('{app}\doubao-remote.exe'));
    if HadNewExe and not FileCopy(ExpandConstant('{app}\doubao-remote.exe'), BackupDir + '\previous-remote.exe', False) then
      RaiseException('无法备份原程序，已停止安装。');
    HadOldRun := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoPTT', OldRunValue);
    HadNewRun := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoRemote', NewRunValue);
    if not SaveStringToFile(BackupDir + '\startup.txt', 'DoubaoPTT=' + OldRunValue + #13#10 + 'DoubaoRemote=' + NewRunValue, False) then RaiseException('无法备份启动项。');
    HadDesktopLink := FileExists(ExpandConstant('{userdesktop}\豆包远程.lnk'));
    HadMenuLink := FileExists(ExpandConstant('{group}\豆包远程.lnk'));
    PreparedAppPath := ExpandConstant('{app}');
    UpgradePrepared := True;
  except
    Result := '无法准备覆盖升级：' + GetExceptionMessage;
  end;
end;

function RestoreUpgrade: Boolean;
begin
  Result := True;
  if HadSettings then begin
    if not FileCopy(BackupDir + '\settings-before.json', UpgradeData + '\settings.json', False) then Result := False;
  end else if FileExists(UpgradeData + '\settings.json') then begin
    if not DeleteFile(UpgradeData + '\settings.json') then Result := False;
  end;
  if HadOldRun then begin
    if not RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoPTT', OldRunValue) then Result := False;
  end else RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoPTT');
  if HadNewRun then begin
    if not RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoRemote', NewRunValue) then Result := False;
  end else RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoRemote');
  if HadNewExe then begin
    if not FileCopy(BackupDir + '\previous-remote.exe', ExpandConstant('{app}\doubao-remote.exe'), False) then Result := False;
  end else if FileExists(ExpandConstant('{app}\doubao-remote.exe')) then begin
    if not DeleteFile(ExpandConstant('{app}\doubao-remote.exe')) then Result := False;
  end;
  if not HadDesktopLink then DeleteFile(ExpandConstant('{userdesktop}\豆包远程.lnk'));
  if not HadMenuLink then DeleteFile(ExpandConstant('{group}\豆包远程.lnk'));
end;

{ 0 missing; 1 present/healthy; 2 reboot requested by Windows; -1 query/device error. }
function CableState(var Details: String): Integer;
var
  Locator, Services, Devices, Device: Variant;
  I, Code: Integer;
begin
  Result := 0;
  Details := '';
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Services := Locator.ConnectServer('', 'root\CIMV2');
    Devices := Services.ExecQuery('SELECT Present, ConfigManagerErrorCode FROM Win32_PnPEntity WHERE Service = ''VBAudioVACMME''');
    for I := 0 to Devices.Count - 1 do begin
      Device := Devices.ItemIndex(I);
      if not VarIsNull(Device.Present) and Device.Present then begin
        Code := Device.ConfigManagerErrorCode;
        if Code = 0 then begin Result := 1; Exit; end;
        if Code = 14 then Result := 2
        else begin
          Details := 'VB-CABLE 设备状态异常，Windows 错误码：' + IntToStr(Code) + '。请先重启电脑，再重新运行本安装程序。';
          Result := -1;
          Exit;
        end;
      end;
    end;
  except
    Details := '无法检查虚拟麦克风：' + GetExceptionMessage;
    Result := -1;
  end;
end;

procedure ReleaseVendorLocks;
var I: Integer;
begin
  for I := GetArrayLength(VendorLocks) - 1 downto 0 do VendorLocks[I].Free;
  SetArrayLength(VendorLocks, 0);
end;

procedure LockFileForInstall(const Name: String);
var N: Integer;
begin
  N := GetArrayLength(VendorLocks);
  SetArrayLength(VendorLocks, N + 1);
  VendorLocks[N] := TFileStream.Create(Name, fmOpenRead or fmShareDenyWrite);
end;

procedure ExtractVerifiedCable;
var
  ZipPath, ManifestPath, Directory, FileName, Hash, Line: String;
  Lines: TStringList;
  I: Integer;
begin
  ExtractTemporaryFile('VBCABLE_Driver_Pack45.zip');
  ExtractTemporaryFile('vbcable-files.sha256');
  ZipPath := ExpandConstant('{tmp}\VBCABLE_Driver_Pack45.zip');
  ManifestPath := ExpandConstant('{tmp}\vbcable-files.sha256');
  LockFileForInstall(ZipPath);
  LockFileForInstall(ManifestPath);
  if CompareText(GetSHA256OfFile(ZipPath), '{#CableZipSHA256}') <> 0 then
    RaiseException('VB-CABLE 安装包校验失败，请重新获取豆包远程安装包。');
  if CompareText(GetSHA256OfFile(ManifestPath), '{#CableManifestSHA256}') <> 0 then
    RaiseException('VB-CABLE 文件清单校验失败。');
  Directory := ExpandConstant('{tmp}\VB-CABLE');
  ForceDirectories(Directory);
  ExtractArchive(ZipPath, Directory, '', True, nil);
  Lines := TStringList.Create;
  try
    Lines.LoadFromFile(ManifestPath);
    for I := 0 to Lines.Count - 1 do begin
      Line := Lines[I];
      if Length(Line) < 67 then RaiseException('VB-CABLE 文件清单格式错误。');
      Hash := Copy(Line, 1, 64);
      FileName := Copy(Line, 67, Length(Line));
      if (FileName = '') or (ExtractFileName(FileName) <> FileName) or (Pos(':', FileName) > 0) then
        RaiseException('VB-CABLE 文件名无效。');
      LockFileForInstall(Directory + '\' + FileName);
      if CompareText(GetSHA256OfFile(Directory + '\' + FileName), Hash) <> 0 then
        RaiseException('VB-CABLE 文件校验失败：' + FileName);
    end;
  finally
    Lines.Free;
  end;
end;

procedure InitializeWizard;
begin
  AudioPage := CreateOutputMsgPage(wpSelectDir, '虚拟麦克风',
    '由安装程序自动准备音频组件',
    '本安装包已包含 VB-CABLE，无需另行下载或手动安装。' + #13#10#13#10 +
    '如果电脑没有安装，点击安装后将出现 Windows 管理员授权提示；同意后自动安装，并在结束时提示重启。' + #13#10#13#10 +
    'VB-CABLE 来源：www.vb-cable.com。它是 VB-Audio 的 donationware，欢迎捐赠或购买许可证：shop.vb-audio.com。' + #13#10 +
    '豆包远程卸载时会保留共享的 VB-CABLE。');
end;

procedure CurPageChanged(CurPageID: Integer);
var Details: String; State: Integer;
begin
  if CurPageID = AudioPage.ID then begin
    State := CableState(Details);
    if State = 1 then AudioPage.Description := '已检测到 VB-CABLE，本次直接复用'
    else if State = 2 then AudioPage.Description := 'VB-CABLE 已安装，Windows 提示需要重启'
    else if State = 0 then AudioPage.Description := '缺少虚拟麦克风，将自动安装 VB-CABLE'
    else AudioPage.Description := '音频组件检查未通过，请查看安装准备阶段的提示';
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var ErrorCode: Integer;
begin
  if CurStep = ssPostInstall then begin
    { The installed executable updates its own settings and HKCU Run together.
      This command does not launch the tray, audio or input agent. }
    try
      if not Exec(ExpandConstant('{app}\doubao-remote.exe'), 'install-defaults',
        ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ErrorCode) then
        RaiseException('无法应用登录启动设置：' + SysErrorMessage(ErrorCode));
      if ErrorCode <> 0 then RaiseException('无法保存设置。错误码：' + IntToStr(ErrorCode));
    except
      if RestoreUpgrade then RaiseException('安装配置失败，已恢复原程序及设置。备份：' + BackupDir + '。原因：' + GetExceptionMessage)
      else RaiseException('安装配置失败，回滚未完成。已保留备份，请人工恢复：' + BackupDir + '。原因：' + GetExceptionMessage);
    end;
    { Remove only the known old product files after new configuration succeeds. }
    DeleteFile(ExpandConstant('{app}\doubao-ptt.exe'));
    DeleteFile(ExpandConstant('{userdesktop}\DoubaoPTT.lnk'));
    DeleteFile(ExpandConstant('{userprograms}\DoubaoPTT\DoubaoPTT.lnk'));
    DeleteFile(ExpandConstant('{userprograms}\DoubaoPTT\Uninstall DoubaoPTT.lnk'));
    RemoveDir(ExpandConstant('{userprograms}\DoubaoPTT'));
    DelTree(BackupDir, True, True, True);
    Log('Current-user startup preference preserved; legacy product entries cleaned.');
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Details: String;
  State, ErrorCode, I: Integer;
  Launched: Boolean;
begin
  Result := PrepareUpgrade;
  if Result <> '' then Exit;
  State := CableState(Details);
  if State = 1 then begin Log('VB-CABLE already installed; skipping vendor setup.'); Exit; end;
  if State = 2 then begin AudioNeedsRestart := True; Exit; end;
  if State = -1 then begin Result := Details; Exit; end;
  try
    try
      WizardForm.PreparingLabel.Caption := '正在自动安装虚拟麦克风，请同意 Windows 管理员授权…';
      ExtractVerifiedCable;
      { Elevate only the original vendor-signed installer; the tray stays per-user. }
      Launched := ShellExec('runas', ExpandConstant('{tmp}\VB-CABLE\VBCABLE_Setup_x64.exe'),
        '-i -h', ExpandConstant('{tmp}\VB-CABLE'), SW_HIDE, ewWaitUntilTerminated, ErrorCode);
      if not Launched then begin
        Result := '无法安装虚拟麦克风。请重新点击安装，并允许 Windows 管理员授权。错误：' + SysErrorMessage(ErrorCode);
        Exit;
      end;
      Log('VB-CABLE vendor setup completed. Result code: ' + IntToStr(ErrorCode));
      State := 0;
      for I := 1 to 30 do begin
        State := CableState(Details);
        if State <> 0 then Break;
        Sleep(250);
      end;
      if (State <> 1) and (State <> 2) then begin
        if Details = '' then Details := '安装后未检测到虚拟麦克风。';
        Result := Details + ' 请重启电脑后重新运行豆包远程安装程序。';
        Exit;
      end;
      { The vendor requires reboot even when endpoints appear immediately. }
      AudioNeedsRestart := True;
    finally
      ReleaseVendorLocks;
    end;
  except
    Result := '虚拟麦克风安装未完成：' + GetExceptionMessage;
  end;
end;

function NeedRestart: Boolean;
begin
  Result := AudioNeedsRestart;
end;

function CanLaunchAfterInstall: Boolean;
begin
  Result := not AudioNeedsRestart;
end;
