use std::path::PathBuf;
use std::time::Duration;

use modlist_core::process::RunningProcess;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{BOOL, PWSTR};

use super::from_wide;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful Win32 call and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Lists running processes whose exe file name satisfies `wanted`. Image
/// paths are only queried for those, which keeps this cheap.
pub fn running(wanted: impl Fn(&str) -> bool) -> Vec<RunningProcess> {
    let mut out = Vec::new();
    // SAFETY: standard ToolHelp enumeration with a correctly sized entry.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let snap = OwnedHandle(snap);
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap.0, &mut entry).is_ok();
        while ok {
            let name = from_wide(&entry.szExeFile);
            if wanted(&name) {
                out.push(RunningProcess { pid: entry.th32ProcessID, path: image_path(entry.th32ProcessID), name });
            }
            ok = Process32NextW(snap.0, &mut entry).is_ok();
        }
    }
    out
}

pub fn image_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: limited query rights; the buffer length is passed in and out.
    unsafe {
        let handle = OwnedHandle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        QueryFullProcessImageNameW(handle.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// Waits for a process to exit. Returns false on timeout.
pub fn wait_for_exit(pid: u32, timeout: Option<Duration>) -> bool {
    // SAFETY: SYNCHRONIZE-only handle used for a single wait.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else { return true };
        let h = OwnedHandle(h);
        let ms = timeout.map_or(u32::MAX, |t| t.as_millis().min(u128::from(u32::MAX - 1)) as u32);
        WaitForSingleObject(h.0, ms) == WAIT_OBJECT_0
    }
}

/// Ends a process, as Task Manager's End task does, and waits up to five
/// seconds for it to go.
pub fn end(pid: u32) -> Result<(), String> {
    // SAFETY: a handle with only terminate and synchronize rights, closed on drop.
    unsafe {
        let h = OwnedHandle(OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid).map_err(|e| e.message())?);
        TerminateProcess(h.0, 1).map_err(|e| e.message())?;
        if WaitForSingleObject(h.0, 5_000) != WAIT_OBJECT_0 {
            return Err("it did not exit within 5 seconds".into());
        }
    }
    Ok(())
}

/// Brings a visible top-level window of `pid` to the front.
pub fn focus_process_window(pid: u32) -> bool {
    struct Search {
        pid: u32,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam points at the Search owned by the caller for the
        // duration of EnumWindows.
        unsafe {
            let search = &mut *(lparam.0 as *mut Search);
            let mut owner = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            if owner == search.pid && IsWindowVisible(hwnd).as_bool() {
                search.found = Some(hwnd);
                return false.into();
            }
            true.into()
        }
    }
    let mut search = Search { pid, found: None };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
        let Some(hwnd) = search.found else { return false };
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}
