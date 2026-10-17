use std::path::PathBuf;

use modlist_core::steam::SteamRoot;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::w;

/// Steam's install folder from `HKCU\Software\Valve\Steam\SteamPath`.
pub fn locate() -> Option<SteamRoot> {
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: buffer size in bytes is passed and updated by the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Valve\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let path = PathBuf::from(super::from_wide(&buf).replace('/', "\\"));
    path.is_dir().then(|| SteamRoot::new(path))
}

pub fn is_running() -> bool {
    !super::processes::running(|n| n.eq_ignore_ascii_case("steam.exe")).is_empty()
}
