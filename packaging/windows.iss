#ifndef AppVersion
#define AppVersion "0.1.1"
#endif
#include "..\artifacts\vendor\vb-cable-45\vbcable.generated.iss"
#ifdef UmdfTrial
#include "..\artifacts\umdf-trial\umdf.generated.iss"
#endif
[Setup]
AppId={{88f8d6f7-3dca-4018-9bf0-d6f4ea807bea}
AppName=豆包麦克风
AppVerName=豆包麦克风
UninstallDisplayName=豆包麦克风
AppVersion={#AppVersion}
OutputBaseFilename=DoubaoMicrophone-{#AppVersion}-windows-x64
DefaultDirName={localappdata}\Programs\DoubaoMicrophone
DefaultGroupName=豆包麦克风
UsePreviousAppDir=yes
UsePreviousGroup=no
PrivilegesRequired=lowest
#ifdef UmdfTrial
DisableWelcomePage=yes
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
#else
DisableDirPage=auto
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
MinVersion=10.0.22000
OutputDir=..\dist
Compression=lzma2
SolidCompression=yes
; ExtractVerifiedCable opens the vendor's original ZIP. The enhanced modes only support 7z.
ArchiveExtraction=full
LicenseFile=..\artifacts\vendor\vb-cable-45\windows-license.txt
SetupIconFile=..\assets\icons\doubao-microphone.ico
UninstallDisplayIcon={app}\doubao-microphone.exe
[Files]
; Preserve the vendor's complete original archive, including its original EULA.
Source: "..\artifacts\vendor\vb-cable-45\VBCABLE_Driver_Pack45.zip"; Flags: dontcopy
Source: "..\artifacts\vendor\vb-cable-45\vbcable-files.sha256"; Flags: dontcopy
#ifdef UmdfTrial
Source: "..\artifacts\umdf-trial\driver\DoubaoMicrophoneUm.dll"; Flags: dontcopy
Source: "..\artifacts\umdf-trial\driver\DoubaoMicrophoneUm.inf"; Flags: dontcopy
Source: "..\artifacts\umdf-trial\driver\DoubaoMicrophoneUm.cat"; Flags: dontcopy
Source: "..\artifacts\umdf-trial\driver\DoubaoMicrophoneUm.cer"; Flags: dontcopy
Source: "..\artifacts\umdf-trial\driver\install-umdf-trial.exe"; Flags: dontcopy
Source: "..\artifacts\umdf-trial\doubao-microphone.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\research\windows-umdf\driver\LICENSE"; DestDir: "{app}"; DestName: "UMDF-LICENSE.txt"; Flags: ignoreversion
#else
Source: "..\dist\doubao-microphone.exe"; DestDir: "{app}"; Flags: ignoreversion
#endif
Source: "..\artifacts\vendor\vb-cable-45\windows-license.txt"; DestDir: "{app}"; DestName: "THIRD-PARTY-LICENSES.txt"; Flags: ignoreversion
[Icons]
Name: "{group}\豆包麦克风"; Filename: "{app}\doubao-microphone.exe"; Parameters: "serve --open"
Name: "{userdesktop}\豆包麦克风"; Filename: "{app}\doubao-microphone.exe"; Parameters: "serve --open"; WorkingDir: "{app}"
[Run]
Filename: "{app}\doubao-microphone.exe"; Parameters: "serve --open"; Description: "打开豆包麦克风设置"; Flags: nowait postinstall skipifsilent; Check: CanLaunchAfterInstall
[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\DoubaoMicrophone\DoubaoMicrophone\data"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "DoubaoMicrophone"; Flags: uninsdeletevalue

[Code]
#ifdef UmdfTrial
const
  TrialDriverRetention = ' 本次安装可能已保留试用驱动和证书，可重试安装或按项目安装说明清理。';
#endif
var
#ifndef UmdfTrial
  AudioPage: TOutputMsgWizardPage;
#endif
  AudioNeedsRestart: Boolean;
  VendorLocks: array of TFileStream;
  UpgradePrepared, HadSettings, HadNewExe, HadNewRun: Boolean;
  UpgradeData, NewRunValue, PreparedAppPath, BackupDir: String;
  HadDesktopLink, HadMenuLink: Boolean;
#ifdef UmdfTrial
  UmdfNeedsRestart: Boolean;
#endif

function PrepareUpgrade: String;
var Locator, Services, Processes, Process: Variant; I, Number: Integer; Path, BackupBase: String;
begin
  Result := '';
  if UpgradePrepared and (PreparedAppPath = ExpandConstant('{app}')) then Exit;
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Services := Locator.ConnectServer('', 'root\CIMV2');
    Processes := Services.ExecQuery('SELECT ExecutablePath FROM Win32_Process WHERE Name = ''doubao-microphone.exe''');
    for I := 0 to Processes.Count - 1 do begin
      Process := Processes.ItemIndex(I);
      if not VarIsNull(Process.ExecutablePath) then begin
        Path := Process.ExecutablePath;
        if CompareText(ExtractFileDir(Path), ExpandConstant('{app}')) = 0 then begin
          Result := '请先从托盘退出正在运行的豆包麦克风，再重新安装。'; Exit;
        end;
      end;
    end;
    UpgradeData := ExpandConstant('{localappdata}\DoubaoMicrophone\DoubaoMicrophone\data');
    BackupBase := ExpandConstant('{localappdata}\DoubaoMicrophone\upgrade-backup\') + GetDateTimeString('yyyymmdd-hhnnss', '-', ':');
    BackupDir := BackupBase;
    Number := 0;
    while DirExists(BackupDir) do begin Number := Number + 1; BackupDir := BackupBase + '-' + IntToStr(Number); end;
    if not ForceDirectories(BackupDir) then RaiseException('无法创建升级备份目录。');
    HadSettings := FileExists(UpgradeData + '\settings.json');
    if HadSettings and not FileCopy(UpgradeData + '\settings.json', BackupDir + '\settings-before.json', False) then
      RaiseException('无法备份原有设置，已停止安装。');
    HadNewExe := FileExists(ExpandConstant('{app}\doubao-microphone.exe'));
    if HadNewExe and not FileCopy(ExpandConstant('{app}\doubao-microphone.exe'), BackupDir + '\previous-remote.exe', False) then
      RaiseException('无法备份原程序，已停止安装。');
    HadNewRun := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoMicrophone', NewRunValue);
    if not SaveStringToFile(BackupDir + '\startup.txt', 'DoubaoMicrophone=' + NewRunValue, False) then RaiseException('无法备份启动项。');
    HadDesktopLink := FileExists(ExpandConstant('{userdesktop}\豆包麦克风.lnk'));
    HadMenuLink := FileExists(ExpandConstant('{group}\豆包麦克风.lnk'));
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
  if HadNewRun then begin
    if not RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoMicrophone', NewRunValue) then Result := False;
  end else RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'DoubaoMicrophone');
  if HadNewExe then begin
    if not FileCopy(BackupDir + '\previous-remote.exe', ExpandConstant('{app}\doubao-microphone.exe'), False) then Result := False;
  end else if FileExists(ExpandConstant('{app}\doubao-microphone.exe')) then begin
    if not DeleteFile(ExpandConstant('{app}\doubao-microphone.exe')) then Result := False;
  end;
  if not HadDesktopLink then DeleteFile(ExpandConstant('{userdesktop}\豆包麦克风.lnk'));
  if not HadMenuLink then DeleteFile(ExpandConstant('{group}\豆包麦克风.lnk'));
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
    RaiseException('VB-CABLE 安装包校验失败，请重新获取豆包麦克风安装包。');
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
#ifdef UmdfTrial
  WizardForm.LicenseLabel1.Caption := '请确认安装内容及下方软件许可。';
  WizardForm.LicenseAcceptedRadio.Caption := '我同意许可、信任证书并安装';
  WizardForm.LicenseNotAcceptedRadio.Caption := '我不同意';
  WizardForm.LicenseMemo.Text :=
    '豆包麦克风' + #13#10#13#10 +
    '安装需要一次 Windows 管理员授权。安装器会安装 UMDF 虚拟键盘，并将其 100 年自签名证书加入本机的受信任根和受信任发布者。缺少 VB-CABLE 时一并安装。' + #13#10#13#10 +
    '证书 SHA-1 指纹：{#UmThumbprint}' + #13#10 +
    '证书有效期至（UTC）：{#UmNotAfter}' + #13#10#13#10 +
    '卸载当前用户的应用会保留机器级驱动与证书。' + #13#10#13#10 +
    WizardForm.LicenseMemo.Text;
  WizardForm.LicenseMemo.SelStart := 0;
#else
  AudioPage := CreateOutputMsgPage(wpSelectDir, '虚拟麦克风',
    '由安装程序自动准备音频组件',
    '本安装包已包含 VB-CABLE，无需另行下载或手动安装。' + #13#10#13#10 +
    '如果电脑没有安装，点击安装后将出现 Windows 管理员授权提示；同意后自动安装，并在结束时提示重启。' + #13#10#13#10 +
    'VB-CABLE 来源：www.vb-cable.com。它是 VB-Audio 的 donationware，欢迎捐赠或购买许可证：shop.vb-audio.com。' + #13#10 +
    '豆包麦克风卸载时会保留共享的 VB-CABLE。');
#endif
end;

procedure CurPageChanged(CurPageID: Integer);
#ifndef UmdfTrial
var Details: String; State: Integer;
#endif
begin
#ifdef UmdfTrial
  if CurPageID = wpLicense then WizardForm.NextButton.Caption := '安装';
#else
  if CurPageID = AudioPage.ID then begin
    State := CableState(Details);
    if State = 1 then AudioPage.Description := '已检测到 VB-CABLE，本次直接复用'
    else if State = 2 then AudioPage.Description := 'VB-CABLE 已安装，Windows 提示需要重启'
    else if State = 0 then AudioPage.Description := '缺少虚拟麦克风，将自动安装 VB-CABLE'
    else AudioPage.Description := '音频组件检查未通过，请查看安装准备阶段的提示';
  end;
#endif
end;

procedure CurStepChanged(CurStep: TSetupStep);
var ErrorCode: Integer;
#ifdef UmdfTrial
    Attempt: Integer;
    Ready: Boolean;
#endif
begin
  if CurStep = ssPostInstall then begin
    { The installed executable updates its own settings and HKCU Run together.
      This command does not launch the tray, audio or input agent. }
    try
#ifdef UmdfTrial
      if not UmdfNeedsRestart then begin
        Ready := False;
        for Attempt := 1 to 12 do begin
          if Exec(ExpandConstant('{app}\doubao-microphone.exe'), 'check-trial-keyboard',
            ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ErrorCode) and (ErrorCode = 0) then begin
            Ready := True;
            Break;
          end;
          Sleep(250);
        end;
        if not Ready then
          RaiseException('当前账户无法访问已安装的 UMDF 虚拟键盘，请重启电脑并重新运行试用安装包。' + TrialDriverRetention);
      end;
#endif
      if not Exec(ExpandConstant('{app}\doubao-microphone.exe'), 'install-defaults',
        ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ErrorCode) then
        RaiseException('无法应用登录启动设置：' + SysErrorMessage(ErrorCode));
      if ErrorCode <> 0 then RaiseException('无法保存设置。错误码：' + IntToStr(ErrorCode));
    except
      if RestoreUpgrade then RaiseException('安装配置失败，已恢复原程序及设置。备份：' + BackupDir + '。原因：' + GetExceptionMessage)
      else RaiseException('安装配置失败，回滚未完成。已保留备份，请人工恢复：' + BackupDir + '。原因：' + GetExceptionMessage);
    end;
    DelTree(BackupDir, True, True, True);
    Log('Current-user startup preference saved.');
  end;
end;

function ConfirmCableInstallation: String;
var Details: String; State, I: Integer;
begin
  for I := 1 to 30 do begin
    State := CableState(Details);
    if State <> 0 then Break;
    Sleep(250);
  end;
  Result := '';
  if (State <> 1) and (State <> 2) then begin
    if Details = '' then Details := '安装后未检测到虚拟麦克风。';
    Result := Details + ' 请重启电脑后重新运行豆包麦克风安装程序。';
  end else
    AudioNeedsRestart := True;
end;

#ifdef UmdfTrial
procedure ExtractVerifiedTrialFile(const Name, Hash: String);
var Path: String;
begin
  ExtractTemporaryFile(Name);
  Path := ExpandConstant('{tmp}\') + Name;
  LockFileForInstall(Path);
  if CompareText(GetSHA256OfFile(Path), Hash) <> 0 then
    RaiseException('UMDF 试用文件校验失败：' + Name);
end;

function PrepareTrialDrivers: String;
var TempPath, Arguments, Details: String; ErrorCode, State: Integer; InstallCable: Boolean;
begin
  Result := '';
  State := CableState(Details);
  if State = -1 then begin Result := Details; Exit; end;
  if State = 2 then AudioNeedsRestart := True;
  InstallCable := State = 0;
  TempPath := ExpandConstant('{tmp}\');
  Arguments := '"' + TempPath + 'DoubaoMicrophoneUm.inf" "' + TempPath + 'DoubaoMicrophoneUm.cer"';
  try
    try
      WizardForm.PreparingLabel.Caption := '正在准备驱动，请同意 Windows 管理员授权…';
      if InstallCable then begin
        ExtractVerifiedCable;
        Arguments := Arguments + ' --install-cable';
      end;
      ExtractVerifiedTrialFile('install-umdf-trial.exe', '{#UmHelperSHA256}');
      ExtractVerifiedTrialFile('DoubaoMicrophoneUm.inf', '{#UmInfSHA256}');
      ExtractVerifiedTrialFile('DoubaoMicrophoneUm.dll', '{#UmDllSHA256}');
      ExtractVerifiedTrialFile('DoubaoMicrophoneUm.cat', '{#UmCatSHA256}');
      ExtractVerifiedTrialFile('DoubaoMicrophoneUm.cer', '{#UmCertificateSHA256}');
      if not ShellExec('runas', TempPath + 'install-umdf-trial.exe',
        Arguments, TempPath, SW_HIDE, ewWaitUntilTerminated, ErrorCode) then
        RaiseException('无法获得管理员授权：' + SysErrorMessage(ErrorCode));
      Log('Trial driver installer exit code: ' + IntToStr(ErrorCode));
      if ErrorCode = $20000001 then
        RaiseException('VB-CABLE 安装程序无法启动或未正常退出。' + TrialDriverRetention);
      if (ErrorCode <> 0) and (ErrorCode <> 3010) then
        RaiseException('Windows 错误 ' + IntToStr(ErrorCode) + '：' + SysErrorMessage(ErrorCode));
      UmdfNeedsRestart := ErrorCode = 3010;
      if InstallCable then begin
        Result := ConfirmCableInstallation;
        if Result <> '' then Result := Result + TrialDriverRetention;
      end;
    finally
      ReleaseVendorLocks;
    end;
  except
    Result := '试用驱动安装未完成：' + GetExceptionMessage;
  end;
end;
#else
function PrepareCable: String;
var
  Details: String;
  State, ErrorCode: Integer;
  Launched: Boolean;
begin
  Result := '';
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
      Result := ConfirmCableInstallation;
    finally
      ReleaseVendorLocks;
    end;
  except
    Result := '虚拟麦克风安装未完成：' + GetExceptionMessage;
  end;
end;
#endif

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
#ifdef UmdfTrial
  if WizardSilent or not WizardForm.LicenseAcceptedRadio.Checked then begin
    Result := '请交互运行此试用安装包，确认本机证书信任和驱动安装。';
    Exit;
  end;
#endif
  Result := PrepareUpgrade;
  if Result <> '' then Exit;
#ifdef UmdfTrial
  Result := PrepareTrialDrivers;
#else
  Result := PrepareCable;
#endif
end;

function NeedRestart: Boolean;
begin
  Result := AudioNeedsRestart;
#ifdef UmdfTrial
  Result := Result or UmdfNeedsRestart;
#endif
end;

function CanLaunchAfterInstall: Boolean;
begin
  Result := not NeedRestart;
end;
