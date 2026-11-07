param(
    [switch]$CheckOnly,
    [switch]$SkipFrontend,
    [switch]$SkipTests,
    [switch]$Offline,
    [switch]$PortableOnly,
    [switch]$UmdfTrial,
    [string]$VbCablePackage
)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if ($UmdfTrial -and $PortableOnly) { throw 'The UMDF trial requires the installer.' }
    if (!$CheckOnly -and !$PortableOnly) {
        $compiler=(Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
        if (!$compiler) { $candidate='C:\Program Files (x86)\Inno Setup 6\ISCC.exe'; if (Test-Path $candidate) { $compiler=$candidate } }
        if (!$compiler) { throw 'Inno Setup 6.7+ missing. Install it to build the full installer, or explicitly use -PortableOnly.' }
        & (Join-Path $PSScriptRoot 'prepare-vbcable.ps1') -PackagePath $VbCablePackage
        if ($UmdfTrial -and !(Test-Path 'artifacts/umdf-trial/umdf.generated.iss')) {
            throw 'Prepare the signed trial payload with scripts/prepare-umdf-trial.ps1 first.'
        }
    }
    if (!$SkipFrontend) {
        Push-Location web
        try {
            $npmArgs=@('ci','--ignore-scripts')
            if ($Offline) { $npmArgs+='--offline' }
            & npm @npmArgs
            if ($LASTEXITCODE) { throw 'npm ci failed' }
            npm run build
            if ($LASTEXITCODE) { throw 'web build failed' }
        } finally { Pop-Location }
    }
    $cargoFlags=@('--locked')
    if ($Offline) { $cargoFlags+='--offline' }
    if ($UmdfTrial) { $cargoFlags+=@('--features','windows-umdf-trial','--target-dir','target/umdf-trial') }
    if ($CheckOnly) {
        & cargo check @cargoFlags --all-targets
        if ($LASTEXITCODE) { throw 'cargo check failed' }
        return
    }
    if (!$SkipTests) {
        & cargo test @cargoFlags --all-targets
        if ($LASTEXITCODE) { throw 'tests failed' }
    }
    & cargo build --release @cargoFlags
    if ($LASTEXITCODE) { throw 'release build failed' }
    New-Item -ItemType Directory -Force dist | Out-Null
    if ($UmdfTrial) {
        Copy-Item target/umdf-trial/release/doubao-microphone.exe artifacts/umdf-trial/ -Force
    } else {
        Copy-Item target/release/doubao-microphone.exe dist/ -Force
    }
    if (!$PortableOnly) {
        $version=[regex]::Match((Get-Content Cargo.toml -Raw),'(?m)^version = "([^"]+)"').Groups[1].Value
        if (!$version) { throw 'Cannot read application version' }
        $innoFlags=@('/DAppVersion='+$version)
        if ($UmdfTrial) { $innoFlags+='/DUmdfTrial=1' }
        & $compiler @innoFlags packaging/windows.iss
        if ($LASTEXITCODE) { throw 'Inno Setup failed' }
    }
    Get-FileHash dist/* -Algorithm SHA256 | Format-Table -AutoSize
} finally { Pop-Location }
