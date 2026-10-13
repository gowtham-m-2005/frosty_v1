# Runs spike D twice: with the hidden logic host (and an idle phase), then
# without it, and compares the memory of the whole process tree.
param([int]$IdleSeconds = 90)

$here = $PSScriptRoot
$exe = Join-Path $here "target\debug\spike-capability.exe"
Remove-Item (Join-Path $here "spike.log") -ErrorAction SilentlyContinue

function Get-TreeMB([int]$RootPid) {
    $all = Get-CimInstance Win32_Process
    $ids = New-Object System.Collections.Generic.HashSet[int]
    [void]$ids.Add($RootPid)
    do {
        $before = $ids.Count
        foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId)) { [void]$ids.Add([int]$p.ProcessId) } }
    } while ($ids.Count -gt $before)
    $sum = ($all | Where-Object { $ids.Contains([int]$_.ProcessId) } | Measure-Object WorkingSetSize -Sum).Sum
    [pscustomobject]@{ Processes = $ids.Count; MB = [math]::Round($sum / 1MB) }
}

"=== run A: view + hidden logic host, idle $IdleSeconds s"
$a = Start-Process -FilePath $exe -ArgumentList "--idle", $IdleSeconds -WorkingDirectory $here -PassThru
Start-Sleep -Seconds 15
$memA = Get-TreeMB $a.Id
"memory, tree of {0} processes: {1} MB (working set, shared pages counted per process)" -f $memA.Processes, $memA.MB
$a | Wait-Process -Timeout ($IdleSeconds + 60) -ErrorAction SilentlyContinue
if (-not $a.HasExited) { "run A did not exit on its own"; Stop-Process -Id $a.Id -Force }

"=== run B: view only, no logic host"
$b = Start-Process -FilePath $exe -ArgumentList "--no-logic", "--idle", "10" -WorkingDirectory $here -PassThru
Start-Sleep -Seconds 8
$memB = Get-TreeMB $b.Id
"memory, tree of {0} processes: {1} MB" -f $memB.Processes, $memB.MB
$b | Wait-Process -Timeout 60 -ErrorAction SilentlyContinue
if (-not $b.HasExited) { Stop-Process -Id $b.Id -Force }

"=== spike.log"
Get-Content (Join-Path $here "spike.log")
