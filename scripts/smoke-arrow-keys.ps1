# Smoke test for moving between cards with the arrow keys. Runs the app with a
# throwaway config of seven fake installs (empty ModOrganizer.exe files that are
# never run), posts key presses straight to the launcher window, and checks
# which card's button has keyboard focus after each one. Exits 1 on a mismatch.
#
#   ./scripts/smoke-arrow-keys.ps1 [-Exe target\release\modlist-launcher.exe]
param([string] $Exe = (Join-Path $PSScriptRoot '..\target\release\modlist-launcher.exe'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class SmokeKeys {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
}
"@
[SmokeKeys]::SetProcessDpiAwarenessContext([IntPtr]-4) | Out-Null

$work = Join-Path ([IO.Path]::GetTempPath()) "modlist-keys-$PID"
$data = Join-Path $work 'data'
New-Item -ItemType Directory -Force $data | Out-Null
$installs = foreach ($n in 1..7) {
    $root = Join-Path $work "List $n"
    New-Item -ItemType Directory -Force (Join-Path $root 'profiles\Default') | Out-Null
    Set-Content -Path (Join-Path $root 'ModOrganizer.exe') -Value '' -NoNewline
    Set-Content -Path (Join-Path $root 'ModOrganizer.ini') -Value "[General]`ngameName=Game $n`n"
    @{ id = [guid]::NewGuid().ToString(); name = "List $n"; exePath = (Join-Path $root 'ModOrganizer.exe'); workingDirectory = $root }
}
@{ schemaVersion = 1; installations = @($installs) } | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $data 'config.json')

# At 1100 px wide the grid has three columns: 1 2 3 / 4 5 6 / 7.
$expect = @(
    @('LEFT', 1), @('UP', 1), @('RIGHT', 2), @('RIGHT', 3), @('DOWN', 6), @('DOWN', 7),
    @('UP', 4), @('END', 7), @('HOME', 1), @('DOWN', 4), @('DOWN', 7), @('RIGHT', 7), @('LEFT', 6)
)
$vk = @{ RIGHT = 0x27; LEFT = 0x25; UP = 0x26; DOWN = 0x28; HOME = 0x24; END = 0x23 }

$psi = [System.Diagnostics.ProcessStartInfo]::new((Resolve-Path $Exe))
$psi.Environment['MODLIST_LAUNCHER_DATA'] = $data
$p = [System.Diagnostics.Process]::Start($psi)
$failed = $false
try {
    $h = [IntPtr]::Zero
    for ($i = 0; $i -lt 50 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 200; $p.Refresh(); $h = $p.MainWindowHandle }
    if ($h -eq [IntPtr]::Zero) { throw 'the launcher window did not appear' }
    [SmokeKeys]::MoveWindow($h, 60, 60, 1100, 800, $true) | Out-Null
    Start-Sleep -Milliseconds 1500
    $byPid = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ProcessIdProperty), $p.Id
    $root = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $byPid)
    Start-Sleep -Milliseconds 500
    $root = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $byPid)
    $focused = New-Object System.Windows.Automation.AndCondition `
        (New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::HasKeyboardFocusProperty), $true), `
        (New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::FrameworkIdProperty), 'Slint')
    $first = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty), 'Open MO2 for List 1'))
    $first.SetFocus()
    Start-Sleep -Milliseconds 500
    foreach ($step in $expect) {
        $key, $want = $step
        $code = $vk[$key]; $scan = [SmokeKeys]::MapVirtualKey($code, 0)
        $lparam = 1 -bor ($scan -shl 16) -bor (1 -shl 24)
        [SmokeKeys]::PostMessage($h, 0x100, [IntPtr]$code, [IntPtr]$lparam) | Out-Null
        [SmokeKeys]::PostMessage($h, 0x101, [IntPtr]$code, [IntPtr]([int64]$lparam -bor (3L -shl 30))) | Out-Null
        Start-Sleep -Milliseconds 800
        $f = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $focused)
        $got = if ($f) { $f.Current.Name } else { '(none)' }
        $ok = $got -eq "Open MO2 for List $want"
        if (-not $ok) { $failed = $true }
        '{0,-6} expected List {1,-2} got {2,-28} {3}' -f $key, $want, $got, $(if ($ok) { 'ok' } else { 'FAIL' })
    }
} finally {
    if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
}
if ($failed) {
    # "(none)" for every step after some point usually means the window lost
    # activation (another window was clicked), not a navigation bug: Windows
    # then reports no focused control at all. Rerun without touching the PC.
    exit 1
}
exit 0
