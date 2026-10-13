# Spike C: summon latency. Starts the app, presses the real global hotkey
# (Ctrl+Alt+F) 50 times with a dismiss in between, then summarizes
# %APPDATA%\frosty\logs\summon-latency.log (hotkey handler to second painted frame).
#
# The overlay flashes on screen for about two minutes. Do not touch the keyboard.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [int]$Runs = 50
)

Add-Type -Namespace W -Name K -MemberDefinition '[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);'

function Send-Hotkey {
    $z = [UIntPtr]::Zero
    [W.K]::keybd_event(0x11, 0, 0, $z)   # Ctrl down
    [W.K]::keybd_event(0x12, 0, 0, $z)   # Alt down
    [W.K]::keybd_event(0x46, 0, 0, $z)   # F down
    Start-Sleep -Milliseconds 40
    [W.K]::keybd_event(0x46, 0, 2, $z)   # F up
    [W.K]::keybd_event(0x12, 0, 2, $z)
    [W.K]::keybd_event(0x11, 0, 2, $z)
}

Get-Process -Name frosty-companion,frosty_companion -ErrorAction SilentlyContinue | Stop-Process -Force
$log = Join-Path $env:APPDATA "frosty\logs\summon-latency.log"
if (Test-Path $log) { Remove-Item $log }

$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 5

for ($i = 0; $i -lt $Runs; $i++) {
    Send-Hotkey            # summon
    Start-Sleep -Milliseconds 1000
    Send-Hotkey            # dismiss
    Start-Sleep -Milliseconds 800
}

Start-Sleep -Seconds 1
Stop-Process -Id $p.Id -Force

if (-not (Test-Path $log)) { "no samples were logged"; exit 1 }
$samples = Get-Content $log | ForEach-Object { [double]$_ } | Sort-Object
$n = $samples.Count
"{0} summons logged out of {1} attempted" -f $n, $Runs
if ($n -eq 0) { exit 1 }
"first (cold):  {0:N1} ms" -f (Get-Content $log | Select-Object -First 1)
"median:        {0:N1} ms" -f $samples[[int][math]::Floor(($n - 1) * 0.5)]
"p95:           {0:N1} ms" -f $samples[[int][math]::Floor(($n - 1) * 0.95)]
"worst:         {0:N1} ms" -f $samples[$n - 1]
"best:          {0:N1} ms" -f $samples[0]
