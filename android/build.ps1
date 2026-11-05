param([switch]$Offline, [switch]$Lint, [switch]$UpdateLocks)
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$oldJava=$env:JAVA_HOME
$oldAndroid=$env:ANDROID_HOME
try {
    if (!$env:JAVA_HOME) {
        $studioJava=Join-Path $env:ProgramFiles 'Android\Android Studio\jbr'
        if (Test-Path (Join-Path $studioJava 'bin\java.exe')) { $env:JAVA_HOME=$studioJava }
    }
    if (!$env:ANDROID_HOME) { $env:ANDROID_HOME=Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
    Push-Location $root
    try {
        $arguments=@('--no-daemon','--console','plain')
        if ($Offline) { $arguments+='--offline' }
        if ($UpdateLocks) { $arguments+='--write-locks' }
        $arguments+=':app:assembleDebug'
        if ($Lint) { $arguments+=':app:lintDebug' }
        New-Item -ItemType Directory -Force 'build' | Out-Null
        $stdout=Join-Path $root 'build\gradle-build.log'
        $stderr=Join-Path $root 'build\gradle-build.err'
        $process=Start-Process -FilePath $env:ComSpec -WorkingDirectory $root -ArgumentList ('/d /c gradlew.bat '+($arguments -join ' ')) -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        $null=$process.Handle
        $process.WaitForExit()
        Get-Content $stdout
        if ((Get-Item $stderr).Length) { Get-Content $stderr }
        if ($process.ExitCode -ne 0) { throw 'Android build failed' }
        $dest=Join-Path (Split-Path -Parent $root) 'dist\android'
        New-Item -ItemType Directory -Force $dest | Out-Null
        $apk=Join-Path $dest 'DoubaoRemote-0.1.0-android-debug.apk'
        Copy-Item 'app\build\outputs\apk\debug\app-debug.apk' $apk -Force
        Get-FileHash $apk -Algorithm SHA256
    } finally { Pop-Location }
} finally { $env:JAVA_HOME=$oldJava;$env:ANDROID_HOME=$oldAndroid }
