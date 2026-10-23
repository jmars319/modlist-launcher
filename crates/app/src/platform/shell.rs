use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile,
    STGM_READ,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Programs, IShellLinkW, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{GUID, HSTRING, Interface};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutPlace {
    Desktop,
    StartMenu,
}

pub fn ensure_com() {
    // SAFETY: initializing COM for this thread; S_FALSE or RPC_E_CHANGED_MODE
    // just mean it is already initialized.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn known_folder(id: &GUID) -> Option<PathBuf> {
    // SAFETY: the returned string is freed with CoTaskMemFree.
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const _));
        path
    }
}

pub fn folder(place: ShortcutPlace) -> Option<PathBuf> {
    match place {
        ShortcutPlace::Desktop => known_folder(&FOLDERID_Desktop),
        ShortcutPlace::StartMenu => known_folder(&FOLDERID_Programs),
    }
}

pub struct Shortcut<'a> {
    pub target: &'a Path,
    pub arguments: &'a str,
    pub working_directory: &'a Path,
    pub icon: Option<(&'a Path, i32)>,
    pub description: &'a str,
}

#[derive(Debug)]
pub enum ShortcutError {
    Exists(PathBuf),
    Failed(String),
}

impl std::fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShortcutError::Exists(p) => write!(f, "{} already exists", p.display()),
            ShortcutError::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// Characters Windows does not allow in file names are replaced.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() { "Modlist".into() } else { trimmed.to_string() }
}

pub fn shortcut_path(place: ShortcutPlace, name: &str) -> Option<PathBuf> {
    Some(folder(place)?.join(format!("{}.lnk", sanitize_file_name(name))))
}

/// Creates a `.lnk`. Refuses to replace an existing file unless `replace`.
pub fn create_shortcut(path: &Path, s: &Shortcut<'_>, replace: bool) -> Result<(), ShortcutError> {
    if path.exists() && !replace {
        return Err(ShortcutError::Exists(path.to_path_buf()));
    }
    ensure_com();
    let fail = |e: windows::core::Error| ShortcutError::Failed(e.message());
    // SAFETY: COM calls on an interface obtained from CoCreateInstance.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(fail)?;
        link.SetPath(&HSTRING::from(s.target.as_os_str())).map_err(fail)?;
        link.SetArguments(&HSTRING::from(s.arguments)).map_err(fail)?;
        link.SetWorkingDirectory(&HSTRING::from(s.working_directory.as_os_str())).map_err(fail)?;
        link.SetDescription(&HSTRING::from(s.description)).map_err(fail)?;
        if let Some((icon, index)) = s.icon {
            link.SetIconLocation(&HSTRING::from(icon.as_os_str()), index).map_err(fail)?;
        }
        let file: IPersistFile = link.cast().map_err(fail)?;
        file.Save(&HSTRING::from(path.as_os_str()), true).map_err(fail)?;
    }
    Ok(())
}

/// A shortcut's target program and arguments.
pub fn read_shortcut(path: &Path) -> Option<(PathBuf, String)> {
    ensure_com();
    // SAFETY: COM calls on an interface obtained from CoCreateInstance, with
    // buffers sized for the strings they receive.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(&HSTRING::from(path.as_os_str()), STGM_READ).ok()?;
        let mut target = vec![0u16; 1024];
        link.GetPath(&mut target, std::ptr::null_mut(), 0).ok()?;
        let mut args = vec![0u16; 4096];
        link.GetArguments(&mut args).ok()?;
        let text =
            |buf: &[u16]| String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())]);
        Some((PathBuf::from(text(&target)), text(&args)))
    }
}

/// The installations that shortcuts directly in `place`'s folder start
/// through this launcher, as written after `--launch` or `--open`.
pub fn launcher_shortcuts(place: ShortcutPlace) -> Vec<String> {
    let Some(dir) = folder(place) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|p| read_shortcut(&p))
        .filter_map(|(target, args)| modlist_core::cli::shortcut_target(&target, &args))
        .collect()
}

pub fn open_in_explorer(path: &Path) -> std::io::Result<()> {
    std::process::Command::new("explorer.exe").arg(path).spawn().map(|_| ())
}
