use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HSTRING, PCWSTR};

use super::shell::ensure_com;

pub enum Pick<'a> {
    Folder,
    /// (filter label, pattern such as `*.exe`)
    File(&'a [(&'a str, &'a str)]),
}

/// Shows the standard Windows open dialog. Returns `None` when cancelled.
pub fn pick(owner: Option<HWND>, title: &str, kind: Pick<'_>, start: Option<&Path>) -> Option<PathBuf> {
    ensure_com();
    // SAFETY: COM dialog usage; strings outlive the calls that borrow them.
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let mut options = dialog.GetOptions().ok()? | FOS_FORCEFILESYSTEM;
        let mut names = Vec::new();
        let mut specs = Vec::new();
        match kind {
            Pick::Folder => options |= FOS_PICKFOLDERS,
            Pick::File(filters) => {
                options |= FOS_FILEMUSTEXIST;
                for (label, pattern) in filters {
                    names.push((HSTRING::from(*label), HSTRING::from(*pattern)));
                }
                for (n, p) in &names {
                    specs.push(COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(p.as_ptr()) });
                }
                dialog.SetFileTypes(&specs).ok()?;
            }
        }
        dialog.SetOptions(options).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        if let Some(dir) = start.filter(|d| d.is_dir())
            && let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(dir.as_os_str()), None)
        {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
        path
    }
}
