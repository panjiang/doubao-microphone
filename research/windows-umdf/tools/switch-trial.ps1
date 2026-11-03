param([switch]$Restore)
$ErrorActionPreference='Stop'
$originalData=Join-Path $env:LOCALAPPDATA 'DoubaoPTT\DoubaoPTT\data'
$trialData=Join-Path $env:LOCALAPPDATA 'DoubaoPTT\UMDF-Test\data'
$trialExe=Join-Path $env:LOCALAPPDATA 'Temp\doubao-ptt-umdf-app-build\target\release\doubao-ptt.exe'
$originalExe=Join-Path $env:LOCALAPPDATA 'Programs\DoubaoPTT\doubao-ptt.exe'
function Api($dir,$path,$method='GET',$body=$null) {
    $url=[Uri](Get-Content (Join-Path $dir 'admin-url.txt') -Raw).Trim()
    $token=[Uri]::UnescapeDataString($url.Fragment.TrimStart('#').Substring(6))
    $parameters=@{Uri=('http://127.0.0.1:38440/api/'+$path);Headers=@{Authorization=('Bearer '+$token)};Method=$method;TimeoutSec=3}
    if ($null -ne $body) { $parameters.Body=($body|ConvertTo-Json -Compress);$parameters.ContentType='application/json' }
    return Invoke-RestMethod @parameters
}
function Stop-Idle($dir,$expected) {
    $status=(Api $dir 'status').status
    if ($status.session_id -or $status.testing -or $status.phase -notin @('idle','disconnected')) { throw 'Active session: refusing to switch applications' }
    [void](Api $dir 'pause' 'PUT' @{paused=$true})
    $listener=Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort 38440 -State Listen
    $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$listener.OwningProcess)
    if ($process.ExecutablePath -ne $expected) { throw 'Listener executable differs from expected path' }
    Stop-Process -Id $process.ProcessId
    Start-Sleep -Milliseconds 500
}
function Start-App($exe,$dir) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$exe
    $info.Arguments='--data-dir "'+$dir+'" serve --open'
    $info.WorkingDirectory=Split-Path $exe
    $info.UseShellExecute=$true
    [void][Diagnostics.Process]::Start($info)
}
if ($Restore) {
    Stop-Idle $trialData $trialExe
    Start-App $originalExe $originalData
    Write-Host 'Original installed application restarted. UMDF trial profile retained separately.'
    exit 0
}
if (!(Test-Path $trialExe)) { throw 'Trial build missing' }
$snapshot=Api $originalData 'config'
$status=(Api $originalData 'status').status
if ($status.session_id -or $status.testing) { throw 'Finish the current session before switching' }
New-Item -ItemType Directory -Force $trialData | Out-Null
foreach ($file in @('ca.pem','ca-key.dpapi','devices.json')) {
    Copy-Item -LiteralPath (Join-Path $originalData $file) -Destination (Join-Path $trialData $file) -Force
}
$snapshot.shortcut='hid_right_alt'
$snapshot.diagnostic_recording=$false
$snapshot.codex_empty_compatibility=$false
$snapshot | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $trialData 'settings.json') -Encoding UTF8
# Rust serde_json expects UTF-8 without BOM.
$settingsPath=Join-Path $trialData 'settings.json'
[IO.File]::WriteAllText($settingsPath,(Get-Content $settingsPath -Raw),[Text.UTF8Encoding]::new($false))
Stop-Idle $originalData $originalExe
try {
    Start-App $trialExe $trialData
    $ready=$false
    for ($i=0;$i -lt 30;$i++) {
        Start-Sleep -Milliseconds 300
        try {
            $setup=Api $trialData 'setup'
            $config=Api $trialData 'config'
            if ($setup.platform.shortcuts -contains 'hid_right_alt' -and $config.shortcut -eq 'hid_right_alt') { $ready=$true;break }
        } catch {}
    }
    if (!$ready) { throw 'Trial application failed readiness check' }
    [ordered]@{trial_ready=$true;shortcut=$config.shortcut;lan_ip=$config.lan_ip;admin_port=$config.admin_port;mobile_port=$config.mobile_port;original_profile_unchanged=$true;ca_identical=((Get-FileHash (Join-Path $trialData 'ca.pem')).Hash -eq (Get-FileHash (Join-Path $originalData 'ca.pem')).Hash)} | ConvertTo-Json
} catch {
    $listener=Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort 38440 -State Listen -ErrorAction SilentlyContinue
    if ($listener) {
        $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$listener.OwningProcess)
        if ($process.ExecutablePath -eq $trialExe) { Stop-Process -Id $process.ProcessId;Start-Sleep -Milliseconds 300 }
    }
    Start-App $originalExe $originalData
    throw
}
