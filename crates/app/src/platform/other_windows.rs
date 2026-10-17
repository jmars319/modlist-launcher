//! Top-level windows of other processes: finding them, reading their text
//! through UI Automation, and minimizing them. Read-only apart from the
//! window's minimized state.

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Subtree};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_SHOWMINNOACTIVE, ShowWindow,
};
use windows::core::BOOL;

use super::shell::ensure_com;

/// Visible top-level windows owned by `pid`, in Z order.
pub fn visible_top_level(pid: u32) -> Vec<HWND> {
    struct Search {
        pid: u32,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam points at the Search owned by the caller for the
        // duration of EnumWindows.
        unsafe {
            let search = &mut *(lparam.0 as *mut Search);
            let mut owner = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            if owner == search.pid && IsWindowVisible(hwnd).as_bool() {
                search.found.push(hwnd);
            }
            true.into()
        }
    }
    let mut search = Search { pid, found: Vec::new() };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// The names UI Automation reports for a window and everything inside it,
/// which for Qt dialogs includes labels and button text.
pub fn texts(hwnd: HWND) -> Vec<String> {
    ensure_com();
    // SAFETY: COM calls on interfaces obtained from CoCreateInstance.
    unsafe {
        let Ok(uia) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else {
            return Vec::new();
        };
        let (Ok(root), Ok(all)) = (uia.ElementFromHandle(hwnd), uia.CreateTrueCondition()) else {
            return Vec::new();
        };
        let Ok(found) = root.FindAll(TreeScope_Subtree, &all) else { return Vec::new() };
        let count = found.Length().unwrap_or(0);
        (0..count)
            .filter_map(|i| found.GetElement(i).ok())
            .filter_map(|e| e.CurrentName().ok())
            .map(|n| n.to_string())
            .filter(|n| !n.is_empty())
            .collect()
    }
}

/// Finds MO2's "waiting on an application to close before exiting" dialog
/// for this game and minimizes it. Other MO2 windows, such as error or
/// warning dialogs, never match and are left alone.
pub fn minimize_mo2_wait_dialog(mo2_pid: u32, game_exe_name: &str, game_pid: u32) -> bool {
    visible_top_level(mo2_pid)
        .into_iter()
        .find(|&hwnd| modlist_core::process::is_mo2_wait_dialog(&texts(hwnd), game_exe_name, game_pid))
        .is_some_and(minimize_quietly)
}

/// Minimizes a window without activating anything, so a game in the
/// foreground keeps focus.
pub fn minimize_quietly(hwnd: HWND) -> bool {
    // SAFETY: plain window-state calls on a window handle.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
        IsIconic(hwnd).as_bool()
    }
}
