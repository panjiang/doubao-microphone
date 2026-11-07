param(
    [Parameter(Mandatory)][string]$DriverBuild,
    [Parameter(Mandatory)][string]$SignTool,
    [Parameter(Mandatory)][string]$Inf2Cat,
    [string]$OutputDirectory,
    [string]$SignerThumbprint,
    [switch]$CreateSigner
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$output = if ($OutputDirectory) { $OutputDirectory } else { Join-Path $root 'artifacts\umdf-trial' }
$driver = Join-Path $output 'driver'
$subject = 'CN=Doubao Remote UMDF Controlled Trial'

if ([bool]$SignerThumbprint -eq [bool]$CreateSigner) {
    throw 'Specify exactly one of -CreateSigner or -SignerThumbprint.'
}
$files = [ordered]@{
    UmDll = 'DoubaoRemoteUm.dll'
    UmInf = 'DoubaoRemoteUm.inf'
    UmCat = 'DoubaoRemoteUm.cat'
    UmCertificate = 'DoubaoRemoteUm.cer'
    UmHelper = 'install-umdf-trial.exe'
}
foreach ($key in @('UmDll','UmInf','UmHelper')) {
    if (!(Test-Path -LiteralPath (Join-Path $DriverBuild $files[$key]) -PathType Leaf)) {
        throw ('Missing driver build input: ' + $files[$key])
    }
}
foreach ($tool in @($SignTool, $Inf2Cat)) {
    if (!(Test-Path -LiteralPath $tool -PathType Leaf)) { throw "Missing tool: $tool" }
}
if ($CreateSigner) {
    $options = @{
        Type = 'CodeSigningCert'
        Subject = $subject
        KeyAlgorithm = 'RSA'
        KeyLength = 3072
        HashAlgorithm = 'SHA256'
        KeyExportPolicy = 'NonExportable'
        NotAfter = (Get-Date).AddYears(100)
        CertStoreLocation = 'Cert:\CurrentUser\My'
    }
    $cert = New-SelfSignedCertificate @options
    Write-Host ('Created signer in CurrentUser\My: ' + $cert.Thumbprint)
} else {
    if ($SignerThumbprint -notmatch '^[a-fA-F0-9]{40}$') { throw 'Invalid signer thumbprint.' }
    $cert = Get-Item -LiteralPath "Cert:\CurrentUser\My\$SignerThumbprint"
}
if ($cert.Subject -ne $subject -or !$cert.HasPrivateKey -or $cert.NotAfter -le (Get-Date)) {
    throw 'Expected an active Doubao Remote trial signing certificate with a local private key.'
}
New-Item -ItemType Directory -Force -Path $driver | Out-Null
foreach ($key in @('UmDll','UmInf','UmHelper')) {
    Copy-Item -LiteralPath (Join-Path $DriverBuild $files[$key]) -Destination $driver -Force
}

function Run-Tool([string]$Tool, [string]$Arguments) {
    $process = Start-Process -FilePath $Tool -ArgumentList $Arguments -WorkingDirectory $driver -NoNewWindow -PassThru
    try {
        $null = $process.Handle
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "$Tool failed with exit code $($process.ExitCode)." }
    } finally {
        $process.Dispose()
    }
}

function Sign-DriverFile([string]$Name) {
    $path = Join-Path $driver $Name
    Run-Tool $SignTool ('sign /s My /sha1 {0} /fd SHA256 "{1}"' -f $cert.Thumbprint, $path)
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.SignerCertificate.Thumbprint -ne $cert.Thumbprint -or
        $signature.Status -notin @('Valid','NotTrusted','UnknownError')) {
        throw "Unexpected signer or signature status for ${Name}: $($signature.Status)"
    }
}

Sign-DriverFile $files.UmDll
Run-Tool $Inf2Cat ('/driver:"{0}" /os:10_CO_X64,10_NI_X64,10_GE_X64' -f $driver)
Sign-DriverFile $files.UmCat
Export-Certificate -Cert $cert -FilePath (Join-Path $driver $files.UmCertificate) -Type CERT -Force | Out-Null

$definitions = foreach ($entry in $files.GetEnumerator()) {
    $hash = (Get-FileHash -LiteralPath (Join-Path $driver $entry.Value) -Algorithm SHA256).Hash
    '#define {0}SHA256 "{1}"' -f $entry.Key, $hash
}
$definitions += '#define UmThumbprint "{0}"' -f $cert.Thumbprint
$definitions += '#define UmNotAfter "{0}"' -f $cert.NotAfter.ToUniversalTime().ToString('yyyy-MM-dd HH:mm:ss')
$definitions | Set-Content -LiteralPath (Join-Path $output 'umdf.generated.iss') -Encoding ASCII
[pscustomobject]@{
    thumbprint = $cert.Thumbprint
    valid_from_utc = $cert.NotBefore.ToUniversalTime().ToString('o')
    valid_until_utc = $cert.NotAfter.ToUniversalTime().ToString('o')
    certificate_sha256 = (Get-FileHash -LiteralPath (Join-Path $driver $files.UmCertificate) -Algorithm SHA256).Hash
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'signing.json') -Encoding ASCII
Get-Content -LiteralPath (Join-Path $output 'signing.json')
