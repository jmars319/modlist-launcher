//! Windows-specific operations. Everything that touches the OS lives here so
//! the rest of the app stays testable.

pub mod clipboard;
pub mod dialogs;
pub mod icons;
pub mod launch;
pub mod processes;
pub mod shell;
pub mod steam;

use std::path::PathBuf;

/// Launcher-owned data folder: `%LOCALAPPDATA%\Modlist Launcher`.
/// `MODLIST_LAUNCHER_DATA` overrides it, which keeps development and test
/// runs away from a real configuration.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MODLIST_LAUNCHER_DATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Modlist Launcher")
}

pub fn shift_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
    // SAFETY: plain query of the async key state.
    unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) as u16 & 0x8000 != 0 }
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
