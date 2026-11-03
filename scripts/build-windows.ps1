param([switch]$CheckOnly,[switch]$SkipFrontend)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if (!$SkipFrontend) {
        Push-Location web
        try { npm ci --ignore-scripts; if ($LASTEXITCODE) { throw 'npm ci failed' }; npm run build; if ($LASTEXITCODE) { throw 'web build failed' } } finally { Pop-Location }
    }
    if ($CheckOnly) { cargo check --locked --all-targets; if ($LASTEXITCODE) { throw 'cargo check failed' }; exit 0 }
    cargo test --locked --all-targets
    if ($LASTEXITCODE) { throw 'tests failed' }
    cargo build --release --locked
    if ($LASTEXITCODE) { throw 'release build failed' }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item target/release/doubao-ptt.exe dist/
    $compiler=(Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
    if (!$compiler) { $candidate='C:\Program Files (x86)\Inno Setup 6\ISCC.exe'; if (Test-Path $candidate) { $compiler=$candidate } }
    if ($compiler) { & $compiler packaging/windows.iss; if ($LASTEXITCODE) { throw 'Inno Setup failed' } }
    else { Write-Warning 'Inno Setup 6 missing; portable EXE built, installer not built.' }
    Get-FileHash dist/* -Algorithm SHA256 | Format-Table -AutoSize
} finally { Pop-Location }
