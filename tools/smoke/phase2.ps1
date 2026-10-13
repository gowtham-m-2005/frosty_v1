# Phase 2 smoke test: runs the real app against a throwaway Frosty Home with
#   - the built-in `hello` capability enabled,
#   - a user capability whose onActivate throws,
#   - a user capability with a broken manifest,
# then checks that hello came up, the broken ones were contained, and the app
# is still running. Needs a debug build and `pnpm build:capabilities`.
#
#   powershell -File tools/smoke/phase2.ps1 -Exe <path to frosty-companion.exe>
param([Parameter(Mandatory = $true)][string]$Exe)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$home_dir = Join-Path ([System.IO.Path]::GetTempPath()) ("frosty-smoke-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force -Path $home_dir | Out-Null

# Config: hello and crashy are enabled with their grants.
@"
[core]
config_version = 1

[capabilities]
enabled = ["hello", "crashy"]
dock_order = ["hello", "crashy"]

[capabilities.permissions.hello]
storage = true

[capabilities.permissions.crashy]
storage = false
"@ | Set-Content (Join-Path $home_dir "frosty.toml") -Encoding utf8

# A capability whose logic throws when activated.
$crashy = Join-Path $home_dir "capabilities\crashy"
New-Item -ItemType Directory -Force -Path (Join-Path $crashy "dist") | Out-Null
@"
[capability]
id = "crashy"
name = "Crashy"
version = "0.0.1"
api_version = "1"
entry = "dist/index.js"
"@ | Set-Content (Join-Path $crashy "capability.toml") -Encoding utf8
'export default { id: "crashy", onActivate() { throw new Error("deliberate crash"); } };' |
    Set-Content (Join-Path $crashy "dist\index.js") -Encoding utf8

# A capability with a manifest that cannot be loaded.
$broken = Join-Path $home_dir "capabilities\broken"
New-Item -ItemType Directory -Force -Path $broken | Out-Null
"[capability]`nid = `"Broken Id`"`nname = `"Broken`"`nversion = `"1`"`napi_version = `"9`"`nentry = `"dist/index.js`"" |
    Set-Content (Join-Path $broken "capability.toml") -Encoding utf8

$env:FROSTY_HOME = $home_dir
$env:FROSTY_BUILTIN_DIR = Join-Path $repo "capabilities"
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 12

$ok = $true
function Check($label, $condition) {
    if ($condition) { "PASS  $label" } else { "FAIL  $label"; $script:ok = $false }
}

Check "app is still running" (-not $p.HasExited)

$log = Join-Path $home_dir "logs\capabilities.log"
$logText = if (Test-Path $log) { Get-Content $log -Raw } else { "" }
Check "hello logic loaded from its folder and ran onActivate" ($logText -match "\[info\] hello: hello is ready")
Check "crashy's activation failure was logged with its message" ($logText -match "\[error\] crashy: .*deliberate crash")

# Inspect the database with the Python that ships on this machine.
$py = @"
import sqlite3, sys, json
db = sqlite3.connect(r'$home_dir\frosty.db')
tables = [r[0] for r in db.execute("select name from sqlite_master where type='table'")]
state = {r[0]: (r[1], r[2]) for r in db.execute("select id, state, last_error from capability_state")}
migrated = [r[0] for r in db.execute("select owner from migrations where owner like 'cap:%'")]
print(json.dumps({"tables": tables, "state": state, "migrated": migrated}))
"@
$info = $py | python - | ConvertFrom-Json
Check "hello's own table was created by its migration" ($info.tables -contains "cap_hello_greetings")
Check "hello's migration is recorded under its own owner" ($info.migrated -contains "cap:hello")
Check "hello is recorded as enabled" ($info.state.hello[0] -eq "enabled")
Check "crashy is recorded as error, not enabled" ($info.state.crashy[0] -eq "error")
Check "crashy's error message is stored" ("$($info.state.crashy[1])" -match "deliberate crash")
Check "the broken manifest did not create a capability" (-not ($info.state.PSObject.Properties.Name -contains "broken"))

# Disabling must also survive: kill and restart, hello comes back from config.
Stop-Process -Id $p.Id -Force
Start-Sleep -Seconds 1
Remove-Item $log -ErrorAction SilentlyContinue
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 10
$logText = if (Test-Path $log) { Get-Content $log -Raw } else { "" }
Check "hello comes back after a restart" ($logText -match "\[info\] hello: hello is ready")
Check "app still running after restart" (-not $p.HasExited)
Stop-Process -Id $p.Id -Force

"home: $home_dir"
if ($ok) { "SMOKE PASS" } else { "SMOKE FAIL"; exit 1 }
