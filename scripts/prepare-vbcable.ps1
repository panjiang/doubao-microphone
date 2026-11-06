param([string]$PackagePath)
# Build preparation only. Does not execute an installer or change system trust.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$spec=Get-Content (Join-Path $root 'packaging\vbcable.lock.json') -Raw | ConvertFrom-Json
$cache=Join-Path $root 'artifacts\vendor\vb-cable-45'
if (!$PackagePath) {
    foreach ($candidate in @((Join-Path $cache $spec.filename),(Join-Path $env:USERPROFILE ('Downloads\'+$spec.filename)),(Join-Path $env:USERPROFILE ('Desktop\'+$spec.filename)))) {
        if (Test-Path -LiteralPath $candidate) { $PackagePath=$candidate;break }
    }
}
if (!$PackagePath -or !(Test-Path -LiteralPath $PackagePath)) {
    throw ('VB-CABLE build input missing. Supply -PackagePath with the official '+$spec.filename+'. Source: '+$spec.source_page+'. No automatic download was attempted.')
}
$PackagePath=(Resolve-Path -LiteralPath $PackagePath).ProviderPath
if ((Get-FileHash -LiteralPath $PackagePath -Algorithm SHA256).Hash -ne $spec.sha256) { throw 'VB-CABLE archive SHA-256 differs from packaging/vbcable.lock.json' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$work=Join-Path ([IO.Path]::GetTempPath()) ('doubao-vbcable-verify-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
try {
    $zip=[IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $PackagePath))
    try {
        $names=@{}
        foreach ($entry in $zip.Entries) {
            $name=$entry.FullName
            if (!$name -or $name -ne [IO.Path]::GetFileName($name) -or $name.Contains(':') -or $names.ContainsKey($name)) { throw 'Unexpected archive entry; package review required' }
            $names[$name]=$true
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Join-Path $work $name),$false)
        }
    } finally { $zip.Dispose() }
    foreach ($expected in $spec.signed_files) {
        $path=Join-Path $work $expected.filename
        if ((Get-FileHash $path -Algorithm SHA256).Hash -ne $expected.sha256) { throw ('File hash mismatch: '+$expected.filename) }
    }
    foreach ($expected in $spec.signed_files) {
        $path=Join-Path $work $expected.filename
        $signaturePath=$path
        if ($expected.catalog) {
            $catalogSpec=@($spec.signed_files | Where-Object { $_.filename -eq $expected.catalog })
            if ($catalogSpec.Count -ne 1 -or $catalogSpec[0].signer_thumbprint -ne $expected.signer_thumbprint) { throw 'Driver catalog must have its own pinned hash and the expected signer' }
            $signaturePath=Join-Path $work $expected.catalog
        }
        $signature=Get-AuthenticodeSignature $signaturePath
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $expected.signer_thumbprint) {
            throw ('Signature validation failed: '+$expected.filename+'; status='+$signature.Status+'; '+$signature.StatusMessage)
        }
        if ($expected.catalog) {
            # Catalog-signed drivers need not contain an embedded signature. Verify
            # membership explicitly without registering the catalog or installing a driver.
            $signTool=(Get-Command signtool.exe -ErrorAction SilentlyContinue).Source
            if (!$signTool) {
                $sdkBin=Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
                if (Test-Path -LiteralPath $sdkBin) {
                    $signTool=Get-ChildItem -LiteralPath $sdkBin -Directory |
                        Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
                        Sort-Object { [version]$_.Name } -Descending |
                        ForEach-Object { Join-Path $_.FullName 'x64\signtool.exe' } |
                        Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
                }
            }
            if (!$signTool) { throw 'Windows SDK signtool.exe is required to verify the catalog-signed VB-CABLE driver' }
            & $signTool verify /v /kp /c $signaturePath $path
            if ($LASTEXITCODE) { throw ('Driver catalog verification failed: '+$expected.filename) }
        }
    }
    New-Item -ItemType Directory -Force $cache | Out-Null
    $cached=Join-Path $cache $spec.filename
    if ([IO.Path]::GetFullPath($PackagePath) -ne [IO.Path]::GetFullPath($cached)) { Copy-Item -LiteralPath $PackagePath -Destination $cached -Force }
    if ((Get-FileHash -LiteralPath $cached -Algorithm SHA256).Hash -ne $spec.sha256) { throw 'Cached archive changed during preparation' }
    $utf8=[Text.UTF8Encoding]::new($false)
    $manifest=(@(Get-ChildItem $work -File | Sort-Object Name | ForEach-Object { (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()+'  '+$_.Name }) -join "`n")+"`n"
    $manifestPath=Join-Path $cache 'vbcable-files.sha256'
    [IO.File]::WriteAllText($manifestPath,$manifest,$utf8)
    $vendorReadme=[Text.Encoding]::GetEncoding(1252).GetString([IO.File]::ReadAllBytes((Join-Path $work 'readme.txt')))
    $notice=@'
VB-CABLE is provided by VB-Audio / Vincent Burel.
Official source: https://www.vb-cable.com/
VB-CABLE is donationware; all participations are welcome.
Support or purchase a license: https://shop.vb-audio.com/
Distribution terms: https://vb-audio.com/Services/licensing.htm

This installer includes the original, unmodified Driver Pack 45 archive.
When needed it invokes the original signed installer with administrator approval.
A Windows restart is required after a new driver installation.
VB-CABLE remains installed when DoubaoRemote is uninstalled because it can be shared.
Professional/organizational deployments must comply with VB-Audio's licensing terms.

The packaged readme predates the current published distribution permission.
The current vendor terms explicitly permit bundling and silent installation
while preserving the vendor identity and donationware model.

Original VB-CABLE readme and end-user terms:
'@
    $license=(Get-Content (Join-Path $root 'LICENSE') -Raw)+"`r`n`r`n"+$notice+"`r`n`r`n"+$vendorReadme
    [IO.File]::WriteAllText((Join-Path $cache 'windows-license.txt'),$license,[Text.UTF8Encoding]::new($true))
    $manifestHash=(Get-FileHash $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $include='#define CableZipSHA256 "'+$spec.sha256+'"'+"`r`n"+'#define CableManifestSHA256 "'+$manifestHash+'"'+"`r`n"
    [IO.File]::WriteAllText((Join-Path $cache 'vbcable.generated.iss'),$include,$utf8)
    [ordered]@{verified=$true;package=$cached;sha256=$spec.sha256;signatures_checked=$spec.signed_files.Count;files=$names.Count;no_driver_installation=$true}|ConvertTo-Json
} finally { Remove-Item -LiteralPath $work -Recurse -Force }
