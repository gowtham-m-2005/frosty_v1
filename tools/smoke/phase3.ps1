# Phase 3 smoke test: Tasks and reminders against the real app, using the
# debug-only FROSTY_SMOKE_CALLS hook and a throwaway Frosty Home.
#
#   1. a reminder comes due: it is delivered with Done and Snooze actions
#   2. completing a repeating task reopens it with a new reminder job
#   3. with do not disturb on, a reminder is held, then delivered after it ends
#   4. a reminder that came due while the app was closed is delivered late
#
# Needs a debug build and `pnpm build:capabilities`. The overlay will appear
# on screen while this runs.
#
#   powershell -File tools/smoke/phase3.ps1 -Exe <path to frosty-companion.exe>
param([Parameter(Mandatory = $true)][string]$Exe)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$home_dir = Join-Path ([System.IO.Path]::GetTempPath()) ("frosty-smoke3-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force -Path $home_dir | Out-Null

@"
[core]
config_version = 1

[notifications]
dnd_schedule_enabled = false

[capabilities]
enabled = ["tasks"]
dock_order = ["tasks"]

[capabilities.permissions.tasks]
storage = true
scheduler = true
notifications = true
"@ | Set-Content (Join-Path $home_dir "frosty.toml") -Encoding utf8

$env:FROSTY_HOME = $home_dir
$env:FROSTY_BUILTIN_DIR = Join-Path $repo "capabilities"

$script:ok = $true
function Check($label, $condition) {
    if ($condition) { "PASS  $label" } else { "FAIL  $label"; $script:ok = $false }
}

function NowMs { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }

function Query($sql) {
    # The SQL goes through a file so quotes in it cannot break the script.
    $sqlFile = Join-Path $home_dir "query.sql"
    Set-Content -Path $sqlFile -Value $sql -Encoding utf8
    $code = @"
import sqlite3, json
db = sqlite3.connect(r'$home_dir\frosty.db')
db.row_factory = sqlite3.Row
sql = open(r'$sqlFile', encoding='utf-8-sig').read()
print(json.dumps([dict(r) for r in db.execute(sql)]))
"@
    $json = $code | python -
    if (-not $json) { return @() }
    return @($json | ConvertFrom-Json)
}

function Run-App($calls, [int]$waitSeconds, [switch]$KillEarly) {
    $callsFile = Join-Path $home_dir "calls.json"
    ConvertTo-Json -InputObject @($calls) -Depth 8 | Set-Content $callsFile -Encoding utf8
    $env:FROSTY_SMOKE_CALLS = $callsFile
    $smokeLog = Join-Path $home_dir "logs\smoke.log"
    Remove-Item $smokeLog -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath $Exe -PassThru

    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $smokeLog) -and ((Get-Content $smokeLog -Raw) -match "done")) { break }
        Start-Sleep -Milliseconds 300
    }
    if (-not $KillEarly) { Start-Sleep -Seconds $waitSeconds }
    $log = if (Test-Path $smokeLog) { Get-Content $smokeLog -Raw } else { "" }
    $alive = -not $p.HasExited
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 800
    [pscustomobject]@{ Log = $log; Alive = $alive }
}

# ---- 1 and 2: a reminder fires; a repeating task reopens ---------------------
$now = NowMs
$first = Run-App @(
    @{ tool = "tasks.add"; input = @{ title = "water plants"; due_at = $now + 6000 } },
    @{ tool = "tasks.add"; input = @{ title = "stretch"; due_at = $now + 3600000; recurrence = @{ kind = "daily" } } },
    @{ tool = "tasks.add"; input = @{ title = "undated" } }
) 9

Check "app stayed up" $first.Alive
Check "task tools answered through the whole stack" ($first.Log -match "ok tasks.add" -and $first.Log -notmatch "err ")

$n = Query "select title, state, actions, delivered_at from notifications where owner = 'tasks'"
Check "the reminder was delivered" (@($n | Where-Object { $_.title -eq "water plants" -and $_.state -eq "delivered" }).Count -eq 1)
$actions = ($n | Where-Object { $_.title -eq "water plants" }).actions
Check "it carries Done and Snooze actions" ($actions -match "tasks.complete" -and $actions -match "tasks.snooze")

$jobs = Query "select owner, action from scheduler_jobs"
Check "the repeating task still has its reminder scheduled" (@($jobs | Where-Object { $_.action -match "tasks.remind" }).Count -ge 1)
Check "the fired reminder's job is gone" (@($jobs | Where-Object { $_.action -match "water" }).Count -eq 0)

# ---- complete the repeating task: it must reopen, not finish -------------------
$items = Query "select id, title, due_at, completed_at from cap_tasks_items where deleted_at is null"
$stretch = $items | Where-Object { $_.title -eq "stretch" }
$before = $stretch.due_at
$second = Run-App @(
    @{ tool = "tasks.complete"; input = @{ id = $stretch.id } }
) 1
Check "completing answered" ($second.Log -match "ok tasks.complete")
$items = Query "select id, title, due_at, completed_at from cap_tasks_items where deleted_at is null"
$after = $items | Where-Object { $_.title -eq "stretch" }
Check "the repeating task is still open" ($null -eq $after.completed_at)
Check "its due date moved forward" ($after.due_at -gt $before)
$completions = Query "select completed_on from cap_tasks_completions where deleted_at is null"
Check "the completion was recorded for the streak" ($completions.Count -ge 1)

# ---- 3: do not disturb holds, then releases ----------------------------------
$py = "import sqlite3; db = sqlite3.connect(r'$home_dir\frosty.db'); db.execute(""insert or replace into kv (key, value, updated_at) values ('dnd.manual', '1', 0)""); db.commit()"
$py | python -
$now = NowMs
$third = Run-App @(
    @{ tool = "tasks.add"; input = @{ title = "while dnd"; due_at = $now + 5000 } }
) 8
$held = Query "select state, hold_reason from notifications where title = 'while dnd'"
Check "with do not disturb on, the reminder was held" (@($held | Where-Object { $_.state -eq "held" -and $_.hold_reason -eq "do_not_disturb" }).Count -eq 1)

$py = "import sqlite3; db = sqlite3.connect(r'$home_dir\frosty.db'); db.execute(""update kv set value = '0' where key = 'dnd.manual'""); db.commit()"
$py | python -
$fourth = Run-App @() 10
$released = Query "select state, delivered_at from notifications where title = 'while dnd'"
Check "after do not disturb ended, the held reminder was delivered" (@($released | Where-Object { $_.state -eq "delivered" }).Count -eq 1)

# ---- 4: due while the app was closed -----------------------------------------
$now = NowMs
$fifth = Run-App @(
    @{ tool = "tasks.add"; input = @{ title = "missed while closed"; due_at = $now + 4000 } }
) 0 -KillEarly
Start-Sleep -Seconds 70   # due while closed, and later than the 60 s a missed run needs to count as late
$sixth = Run-App @() 8
$late = Query "select title, body, state from notifications where title = 'missed while closed'"
Check "a reminder that came due while closed was delivered at the next start" (@($late | Where-Object { $_.state -eq "delivered" }).Count -eq 1)
Check "it says it was due while Frosty was away" (@($late | Where-Object { $_.body -match "while Frosty was away" }).Count -eq 1)

"home: $home_dir"
if ($script:ok) { "SMOKE PASS" } else { "SMOKE FAIL"; exit 1 }
