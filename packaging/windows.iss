#ifndef AppVersion
#define AppVersion "0.2.5"
#endif
#include "..\artifacts\vendor\vb-cable-45\vbcable.generated.iss"
[Setup]
AppId={{BCF882D6-2071-4E38-A957-54E5805439F7}
AppName=DoubaoPTT
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\DoubaoPTT
DefaultGroupName=DoubaoPTT
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
OutputDir=..\dist
OutputBaseFilename=DoubaoPTT-Setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
; ExtractVerifiedCable opens the vendor's original ZIP. The enhanced modes only support 7z.
ArchiveExtraction=full
LicenseFile=..\artifacts\vendor\vb-cable-45\windows-license.txt
SetupIconFile=..\assets\icons\doubao-ptt.ico
UninstallDisplayIcon={app}\doubao-ptt.exe
[Files]
; Preserve the vendor's complete original archive, including its original EULA.
Source: "..\artifacts\vendor\vb-cable-45\VBCABLE_Driver_Pack45.zip"; Flags: dontcopy
Source: "..\artifacts\vendor\vb-cable-45\vbcable-files.sha256"; Flags: dontcopy
Source: "..\dist\doubao-ptt.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\artifacts\vendor\vb-cable-45\windows-license.txt"; DestDir: "{app}"; DestName: "THIRD-PARTY-LICENSES.txt"; Flags: ignoreversion
[Icons]
Name: "{group}\DoubaoPTT"; Filename: "{app}\doubao-ptt.exe"; Parameters: "serve --open"
Name: "{userdesktop}\DoubaoPTT"; Filename: "{app}\doubao-ptt.exe"; Parameters: "serve --open"; WorkingDir: "{app}"
[Run]
Filename: "{app}\doubao-ptt.exe"; Parameters: "serve --open"; Description: "Open DoubaoPTT setup"; Flags: nowait postinstall skipifsilent; Check: CanLaunchAfterInstall
[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\DoubaoPTT\DoubaoPTT\data"
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "DoubaoPTT"; Flags: uninsdeletevalue

[Code]
var
  AudioPage: TOutputMsgWizardPage;
  AudioNeedsRestart: Boolean;
  VendorLocks: array of TFileStream;

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
    RaiseException('VB-CABLE 安装包校验失败，请重新获取 DoubaoPTT 安装包。');
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
    'DoubaoPTT 卸载时会保留共享的 VB-CABLE。');
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
    if not Exec(ExpandConstant('{app}\doubao-ptt.exe'), 'install-defaults',
      ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ErrorCode) then
      RaiseException('无法启用开机自动启动：' + SysErrorMessage(ErrorCode));
    if ErrorCode <> 0 then
      RaiseException('无法保存开机自动启动设置，请重新安装。错误码：' + IntToStr(ErrorCode));
    Log('Current-user login startup enabled by installed executable.');
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Details: String;
  State, ErrorCode, I: Integer;
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
      State := 0;
      for I := 1 to 30 do begin
        State := CableState(Details);
        if State <> 0 then Break;
        Sleep(250);
      end;
      if (State <> 1) and (State <> 2) then begin
        if Details = '' then Details := '安装后未检测到虚拟麦克风。';
        Result := Details + ' 请重启电脑后重新运行 DoubaoPTT 安装程序。';
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
