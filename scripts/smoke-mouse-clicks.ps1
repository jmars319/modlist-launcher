# Smoke test for real mouse clicks on the themed controls: a button, a
# switch, an action row. UI Automation's Invoke skips the mouse path, so a
# control can pass the other tests while ignoring clicks. Posts mouse
# messages straight to the launcher window (the real cursor never moves)
# against a throwaway config with one fake install, and exits 1 if a click
# does not act.
#
#   ./scripts/smoke-mouse-clicks.ps1 [-Exe target\release\modlist-launcher.exe]
param([string] $Exe = (Join-Path $PSScriptRoot '..\target\release\modlist-launcher.exe'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class SmokeClicks {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
}
"@
[SmokeClicks]::SetProcessDpiAwarenessContext([IntPtr]-4) | Out-Null
$A = [System.Windows.Automation.AutomationElement]

$work = Join-Path ([IO.Path]::GetTempPath()) "modlist-clicks-$PID"
$data = Join-Path $work 'data'
$root = Join-Path $work 'Click List'
New-Item -ItemType Directory -Force $data, (Join-Path $root 'profiles\Default') | Out-Null
Set-Content -Path (Join-Path $root 'ModOrganizer.exe') -Value '' -NoNewline
Set-Content -Path (Join-Path $root 'ModOrganizer.ini') -Value "[General]`ngameName=Smoke Test Game`n"
@{ schemaVersion = 1; installations = @(@{ id = [guid]::NewGuid().ToString(); name = 'Click List'; exePath = (Join-Path $root 'ModOrganizer.exe'); workingDirectory = $root }) } |
    ConvertTo-Json -Depth 4 | Set-Content (Join-Path $data 'config.json')

$psi = [System.Diagnostics.ProcessStartInfo]::new((Resolve-Path $Exe))
$psi.Environment['MODLIST_LAUNCHER_DATA'] = $data
$p = [System.Diagnostics.Process]::Start($psi)
$failed = $false
try {
    $h = [IntPtr]::Zero
    for ($i = 0; $i -lt 50 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 200; $p.Refresh(); $h = $p.MainWindowHandle }
    if ($h -eq [IntPtr]::Zero) { throw 'the launcher window did not appear' }
    [SmokeClicks]::MoveWindow($h, 60, 60, 1000, 720, $true) | Out-Null
    Start-Sleep -Milliseconds 1500
    $byPid = New-Object System.Windows.Automation.PropertyCondition ($A::ProcessIdProperty), $p.Id

    # The automation tree can come back empty on the first query.
    function All {
        for ($k = 0; $k -lt 10; $k++) {
            $r = $A::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $byPid)
            $all = @($r.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
            if ($all.Count) { return $all }
            Start-Sleep -Milliseconds 500
        }
        @()
    }
    function Has($pattern) { [bool](All | Where-Object { $_.Current.Name -like $pattern }) }
    function Click($name) {
        $e = All | Where-Object { $_.Current.Name -eq $name } | Select-Object -First 1
        if (-not $e) { throw "no control named '$name'" }
        $b = $e.Current.BoundingRectangle
        $pt = New-Object SmokeClicks+POINT
        $pt.X = [int]($b.X + $b.Width / 2); $pt.Y = [int]($b.Y + $b.Height / 2)
        [SmokeClicks]::ScreenToClient($h, [ref]$pt) | Out-Null
        $l = [IntPtr](($pt.Y -shl 16) -bor ($pt.X -band 0xffff))
        [SmokeClicks]::PostMessage($h, 0x0200, [IntPtr]0, $l) | Out-Null   # WM_MOUSEMOVE
        Start-Sleep -Milliseconds 120
        [SmokeClicks]::PostMessage($h, 0x0201, [IntPtr]1, $l) | Out-Null   # WM_LBUTTONDOWN
        Start-Sleep -Milliseconds 60
        [SmokeClicks]::PostMessage($h, 0x0202, [IntPtr]0, $l) | Out-Null   # WM_LBUTTONUP
        Start-Sleep -Milliseconds 1200
    }
    function Check($label, [bool] $ok) {
        if (-not $ok) { $script:failed = $true }
        '{0,-46} {1}' -f $label, $(if ($ok) { 'ok' } else { 'FAIL' })
    }
    function Setting { (Get-Content (Join-Path $data 'config.json') -Raw | ConvertFrom-Json).settings.closeAfterLaunch }

    Click 'Settings'
    Check 'button: Settings opens the sheet' (Has 'Done')
    $before = [bool](Setting)
    Click 'Close the launcher after starting a modlist'
    Check 'switch: toggles and saves' ([bool](Setting) -ne $before)
    # A click must also move keyboard focus, so Space and Tab continue from it.
    $switch = All | Where-Object { $_.Current.Name -eq 'Close the launcher after starting a modlist' } | Select-Object -First 1
    Check 'switch: click gives it keyboard focus' ([bool]$switch -and $switch.Current.HasKeyboardFocus)
    Click 'Done'
    Check 'button: Done closes the sheet' (-not (Has 'Done'))
    Click 'More actions for Click List'
    $edit = (All | Where-Object { $_.Current.Name -like 'Edit*' } | Select-Object -First 1)
    Check 'button: More opens the actions sheet' ([bool]$edit)
    if ($edit) {
        Click $edit.Current.Name
        Check 'action row: Edit opens the edit sheet' (Has 'Save*')
    }
} finally {
    if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
}
if ($failed) { exit 1 }
exit 0
