param([ValidateSet('Inspect','Install','Remove')][string]$Action='Inspect')
# Authorized research install only. Never changes BCD, firmware, VBS or proxies.
$ErrorActionPreference='Stop'
$package=Join-Path (Split-Path $PSScriptRoot) 'out\package'
$manifest=Get-Content (Join-Path $package 'manifest.json') -Raw | ConvertFrom-Json
$hardware='ROOT\DOUBAOPTT_UMDF_RESEARCH'
if ($manifest.schema -ne 1 -or $manifest.hardware_id -ne $hardware) { throw 'Unexpected manifest' }
$ledgerPath=Join-Path (Split-Path $PSScriptRoot) 'out\installation-ledger.json'
function Save($value) { $value | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ledgerPath -Encoding UTF8 }
function Native([string]$File,[string]$Arguments) {
    $info=[Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$File; $info.Arguments=$Arguments
    $info.UseShellExecute=$false; $info.RedirectStandardOutput=$true
    $process=[Diagnostics.Process]::new();$process.StartInfo=$info
    try {
        [void]$process.Start();$text=$process.StandardOutput.ReadToEnd();$process.WaitForExit()
        return [pscustomobject]@{Code=$process.ExitCode;Text=$text}
    } finally { $process.Dispose() }
}
function Devices { return @(Get-CimInstance Win32_PnPEntity -Filter "PNPDeviceID LIKE 'ROOT%'" | Where-Object { @($_.HardwareID) -contains $hardware }) }
foreach ($entry in $manifest.sha256.PSObject.Properties) {
    if ($entry.Name -ne [IO.Path]::GetFileName($entry.Name)) { throw 'Unsafe manifest entry' }
    if ((Get-FileHash (Join-Path $package $entry.Name) -Algorithm SHA256).Hash -ne $entry.Value) { throw ('Hash mismatch: '+$entry.Name) }
}
$public=Join-Path $package 'DoubaoPTTUm.cer'
$cert=[Security.Cryptography.X509Certificates.X509Certificate2]::new($public)
if ($cert.Thumbprint -ne $manifest.certificate_thumbprint) { throw 'Certificate mismatch' }
$secure=Confirm-SecureBootUEFI
$boot=Native "$env:SystemRoot\System32\bcdedit.exe" '/enum {current}'
if ($boot.Code -ne 0) { throw 'Cannot inspect BCD state' }
$testMode=$boot.Text -match '(?im)^\s*testsigning\s+(Yes|On|1)\s*$'
$existing=@(Devices)
$root='Cert:\LocalMachine\Root\'+$cert.Thumbprint
$publisher='Cert:\LocalMachine\TrustedPublisher\'+$cert.Thumbprint
[ordered]@{action=$Action;secure_boot=$secure;testsigning=$testMode;hashes_valid=$true;certificate_thumbprint=$cert.Thumbprint;root_trusted=(Test-Path $root);publisher_trusted=(Test-Path $publisher);device_count=$existing.Count} | ConvertTo-Json
if ($Action -eq 'Inspect') { exit 0 }
$pnputil="$env:SystemRoot\System32\pnputil.exe"
if ($Action -eq 'Remove') {
    $ledger=Get-Content $ledgerPath -Raw | ConvertFrom-Json
    if ($ledger.hardware_id -ne $hardware -or $ledger.thumbprint -ne $cert.Thumbprint) { throw 'Ledger identity mismatch' }
    if ($existing.Count -gt 1) { throw 'Multiple devices; inspect before removing' }
    if ($existing.Count -eq 1) {
        if ($existing[0].PNPDeviceID -ne $ledger.instance_id) { throw 'Instance mismatch' }
        $r=Native $pnputil ('/remove-device "'+$ledger.instance_id+'"');Write-Host $r.Text
        if ($r.Code -notin @(0,3010)) { throw 'Device removal failed' }
    }
    if ($ledger.oem_inf -match '^oem\d+\.inf$') {
        $driver=Get-WindowsDriver -Online -Driver $ledger.oem_inf
        if ($driver.ProviderName -ne 'DoubaoPTT Research' -or [IO.Path]::GetFileName($driver.OriginalFileName) -ne 'doubaopttum.inf') { throw 'Driver ownership mismatch' }
        $r=Native $pnputil ('/delete-driver '+$ledger.oem_inf);Write-Host $r.Text
        if ($r.Code -notin @(0,3010)) { throw 'Driver package removal failed' }
    }
    if (!$ledger.root_preexisted) { Remove-Item -LiteralPath $root -ErrorAction SilentlyContinue }
    if (!$ledger.publisher_preexisted) { Remove-Item -LiteralPath $publisher -ErrorAction SilentlyContinue }
    $ledger.phase='removed';Save $ledger
    exit 0
}
if (!$secure -or $testMode) { throw 'This experiment requires Secure Boot ON and TESTSIGNING OFF' }
if ($existing.Count -or (Test-Path $ledgerPath)) { throw 'Existing research state; inspect ledger before continuing' }
$ledger=[ordered]@{schema=1;hardware_id=$hardware;thumbprint=$cert.Thumbprint;root_preexisted=(Test-Path $root);publisher_preexisted=(Test-Path $publisher);oem_inf='';instance_id='';phase='prepared';secure_boot_before=$secure;testsigning_before=$testMode}
Save $ledger
[void](Import-Certificate -FilePath $public -CertStoreLocation Cert:\LocalMachine\Root)
[void](Import-Certificate -FilePath $public -CertStoreLocation Cert:\LocalMachine\TrustedPublisher)
$ledger.phase='certificate-imported';Save $ledger
foreach ($name in @('DoubaoPTTUm.dll','DoubaoPTTUm.cat')) {
    $signature=Get-AuthenticodeSignature (Join-Path $package $name)
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $cert.Thumbprint) { throw ('Signature verification failed: '+$name) }
}
$inf=Join-Path $package 'DoubaoPTTUm.inf'
$r=Native $pnputil ('/add-driver "'+$inf+'"');Write-Host $r.Text
if ($r.Code -notin @(0,3010)) { throw 'Driver staging failed; use exact ledger for rollback' }
$oem=[regex]::Matches($r.Text,'(?i)\boem\d+\.inf\b') | Select-Object -First 1
if (!$oem) { throw 'Cannot identify OEM INF' }
$ledger.oem_inf=$oem.Value.ToLowerInvariant();$ledger.phase='staged';Save $ledger
$devgen='C:\Users\Administrator\AppData\Local\Temp\doubao-ptt-vhf-research\out\toolchain\wdk\c\tools\10.0.26100.0\x64\devgen.exe'
if ((Get-AuthenticodeSignature $devgen).Status -ne 'Valid') { throw 'WDK DevGen signature invalid' }
$r=Native $devgen ('/add /bus ROOT /hardwareid '+$hardware);Write-Host $r.Text
$created=@(Devices)
for ($i=0;$created.Count -eq 0 -and $i -lt 30;$i++) { Start-Sleep -Milliseconds 100;$created=@(Devices) }
if ($created.Count -eq 1) { $ledger.instance_id=$created[0].PNPDeviceID;$ledger.phase='device-created';Save $ledger }
if ($r.Code -notin @(0,3010) -or $created.Count -ne 1) { throw 'Device creation failed; ledger retained' }
$r=Native $pnputil ('/add-driver "'+$inf+'" /install');Write-Host $r.Text
if ($r.Code -notin @(0,3010)) { throw 'Driver bind failed; ledger retained' }
$ledger.phase='installed';Save $ledger
$created=@(Devices)
$created | Select-Object Name,PNPDeviceID,ConfigManagerErrorCode | ConvertTo-Json
Write-Host 'UMDF research installed; Secure Boot and BCD were not modified.'
