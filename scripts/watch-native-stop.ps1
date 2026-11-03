param(
    [Parameter(Mandatory=$true)][string]$TraceExe,
    [ValidateRange(0,120)][int]$Minutes=0
)
# Temporary developer diagnostics. No service, startup entry, or app changes.
# Idle: no debugger attached. Recording: a bounded trace, detached at end.
$ErrorActionPreference='Stop'
$dataRoot="$env:LOCALAPPDATA\DoubaoPTT\DoubaoPTT\data"
$nativeRoot=Join-Path $dataRoot 'diagnostics\native-traces'
$null=New-Item -ItemType Directory -Force $nativeRoot
$stopFile=Join-Path $nativeRoot 'watch.stop'
$stateFile=Join-Path $nativeRoot 'watch-state.json'
$mutex=New-Object System.Threading.Mutex($false,'Local\DoubaoPTT.NativeTraceWatch')
if (-not $mutex.WaitOne(0)) {throw 'A native trace watcher is already running.'}
$traceProcess=$null
$childStop=$null
$state='armed'
$deadline=if ($Minutes) {[DateTime]::UtcNow.AddMinutes($Minutes)}else{[DateTime]::MaxValue}
function Save-State([string]$value,[string]$session='',[string]$directory='') {
    @{state=$value;session_id=$session;trace_dir=$directory;until_utc=$(if ($Minutes) {$deadline.ToString('o')}else{$null});watcher_pid=$PID} |
        ConvertTo-Json | Set-Content -Encoding UTF8 -LiteralPath $stateFile
}
function Get-Status {
    (Invoke-RestMethod -Uri "$base/api/status" -Headers $headers -TimeoutSec 2).status
}
function Rotate-Traces([string]$prefix) {
    Get-ChildItem -LiteralPath $nativeRoot -Directory -Filter "$prefix-*" |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -Skip 3 |
        Remove-Item -Recurse -Force
}
try {
    if (Test-Path -LiteralPath $stopFile) {Remove-Item -LiteralPath $stopFile}
    $uri=[uri](Get-Content -Raw (Join-Path $dataRoot 'admin-url.txt')).Trim()
    if ($uri.Host -ne '127.0.0.1' -or $uri.Port -ne 38440) {throw 'Unexpected management address.'}
    $base=$uri.GetLeftPart([System.UriPartial]::Authority)
    $headers=@{Authorization='Bearer '+$uri.Fragment.Substring(1).Replace('token=','')}
    $bridgeConnection=Get-NetTCPConnection -LocalPort $uri.Port -State Listen | Select-Object -First 1
    $bridgeProcess=Get-Process -Id $bridgeConnection.OwningProcess
    $handledSession=''
    Save-State 'armed'
    Write-Output 'Watcher armed. Idle sessions are not attached; exits on captured failure, app exit, stop marker, or deadline.'
    while ([DateTime]::UtcNow -lt $deadline -and -not (Test-Path -LiteralPath $stopFile) -and -not $bridgeProcess.HasExited) {
        $status=Get-Status
        if ($status.session_id -and $status.session_id -ne $handledSession -and $status.phase -ne 'draining') {
            $session=[string]$status.session_id
            $handledSession=$session
            $ime=Get-Process -Name ImeService
            if (@($ime).Count -ne 1) {throw 'Expected one ImeService.'}
            $dir=Join-Path $nativeRoot "pending-$session"
            $null=New-Item -ItemType Directory -Force $dir
            $log=Join-Path $dir 'trace.jsonl'
            $errors=Join-Path $dir 'stderr.log'
            $childStop=Join-Path $dir 'stop'
            Save-State 'tracing' $session $dir
            $arguments=@([string]$ime.Id,'60',('"'+$childStop+'"'))
            $traceProcess=Start-Process -FilePath $TraceExe -ArgumentList $arguments -NoNewWindow -PassThru -RedirectStandardOutput $log -RedirectStandardError $errors
            # Cache the process handle before it exits. Otherwise Windows
            # PowerShell can expose a null ExitCode after a successful wait.
            $traceHandle=$traceProcess.Handle
            while (-not $traceProcess.WaitForExit(100)) {
                $current=Get-Status
                if ($current.session_id -ne $session -or (Test-Path -LiteralPath $stopFile) -or [DateTime]::UtcNow -ge $deadline -or $bridgeProcess.HasExited) {
                    $null=New-Item -ItemType File -Force -Path $childStop
                }
            }
            $traceProcess.WaitForExit()
            if ($null -ne $traceProcess.ExitCode -and $traceProcess.ExitCode -ne 0) {throw "Native tracer failed (exit $($traceProcess.ExitCode)); inspect $errors"}
            $traceProcess=$null
            if (-not ((Get-Content -Raw -LiteralPath $log) -match 'detached=true')) {throw 'Trace detach confirmation missing.'}
            $report=$null
            for ($retry=0;$retry -lt 10 -and -not $report;$retry++) {
                $audioDir=Get-ChildItem -LiteralPath (Join-Path $dataRoot 'diagnostics') -Directory -Filter "*-$session" | Select-Object -First 1
                if ($audioDir -and (Test-Path (Join-Path $audioDir.FullName 'report.json'))) {
                    $report=Get-Content -Raw (Join-Path $audioDir.FullName 'report.json') | ConvertFrom-Json
                } else {Start-Sleep -Milliseconds 100}
            }
            @{session_id=$session;reason=$report.reason;audio_dir=$audioDir.FullName;finished_utc=[DateTime]::UtcNow.ToString('o')} |
                ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $dir 'summary.json')
            $category=if ($report -and -not $report.complete) {'fault'}else{'recent'}
            $destination=Join-Path $nativeRoot "$category-$session"
            Move-Item -LiteralPath $dir -Destination $destination
            Rotate-Traces $category
            if ($report.reason -eq 'doubao_stopped') {
                $state='fault_captured'
                Save-State $state $session $destination
                Write-Output ('Failure captured with native trace: '+$destination)
                break
            }
            Save-State 'armed'
        }
        Start-Sleep -Milliseconds 100
    }
    if ($state -ne 'fault_captured') {$state='finished';Save-State $state}
} catch {
    $state='error'
    Save-State $state
    Write-Error $_
} finally {
    # Request a clean detach; never kill an attached native tracer.
    if ($traceProcess -and -not $traceProcess.HasExited) {
        if ($childStop) {$null=New-Item -ItemType File -Force -Path $childStop}
        $traceProcess.WaitForExit()
    }
    $mutex.ReleaseMutex()
    $mutex.Dispose()
}
