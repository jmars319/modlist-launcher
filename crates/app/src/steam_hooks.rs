//! Writing to and restoring a Steam account's `localconfig.vdf`. Shared by the
//! Steam Play Button sheet, the Remove flow and `--restore-steam`.
//!
//! Every write needs Steam closed, saves a backup in the launcher's data
//! folder first, replaces the file atomically and verifies it by reading back.

use std::path::{Path, PathBuf};

use modlist_core::config::{DataDir, SteamHook, timestamp};
use modlist_core::steam::{self, RestorePlan, SteamRoot};

/// Saves a copy of Steam's settings before changing them.
pub fn backup(data: &DataDir, account_id: u32, text: &str) -> Result<PathBuf, String> {
    let rel = PathBuf::from("steam-backups").join(format!("localconfig-{account_id}-{}.vdf", timestamp()));
    data.write_atomic(&rel, text.as_bytes())
        .map_err(|e| format!("Could not back up Steam's settings, so nothing was changed: {e}"))
}

/// Replaces Steam's settings file through a temporary file and a rename.
pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("vdf.modlist-launcher-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Restored {
    /// The game's previous launch options are back.
    Restored,
    /// The value was changed outside the launcher, so it was left alone.
    LeftAsIs,
}

#[derive(Debug)]
pub enum RestoreError {
    /// Steam rewrites its settings when it exits, so nothing can be changed
    /// while it runs.
    SteamRunning,
    Failed(String),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::SteamRunning => {
                write!(f, "Steam is running. Exit Steam completely (Steam menu > Exit), then try again.")
            }
            RestoreError::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// Undoes a Steam Play hook: puts back the launch options the game had before,
/// unless someone changed them since. Callers pass whether Steam is running
/// (`platform::steam::is_running()`), which keeps this testable.
pub fn restore(
    hook: &SteamHook,
    steam: Option<&SteamRoot>,
    data: &DataDir,
    steam_running: bool,
    log: impl Fn(&str),
) -> Result<Restored, RestoreError> {
    if steam_running {
        return Err(RestoreError::SteamRunning);
    }
    let fail = RestoreError::Failed;
    let steam = steam.ok_or_else(|| fail("Steam was not found on this PC.".into()))?;
    let account = steam
        .accounts()
        .into_iter()
        .find(|a| a.account_id == hook.account_id)
        .ok_or_else(|| fail(format!("Steam account {} was not found on this PC.", hook.account_id)))?;
    let text = std::fs::read_to_string(&account.localconfig)
        .map_err(|e| fail(format!("Could not read Steam's settings: {e}")))?;
    let plan = steam::plan_restore(&text, hook.app_id, &hook.applied_launch_options, &hook.previous_launch_options)
        .map_err(|e| fail(e.to_string()))?;
    let RestorePlan::Restore(updated) = plan else { return Ok(Restored::LeftAsIs) };

    let saved = backup(data, account.account_id, &text).map_err(fail)?;
    write(&account.localconfig, &updated).map_err(|e| fail(format!("Could not write Steam's settings: {e}")))?;
    let check = std::fs::read_to_string(&account.localconfig)
        .ok()
        .and_then(|t| steam::get_launch_options(&t, hook.app_id).ok());
    if check.as_deref() != Some(hook.previous_launch_options.as_str()) {
        let _ = write(&account.localconfig, &text);
        return Err(fail("Steam's settings did not read back correctly, so the original file was put back.".into()));
    }
    log(&format!("steam: restored launch options for app {}; backup {}", hook.app_id, saved.display()));
    Ok(Restored::Restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"1091500\"\n\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"-skipStartScreen\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n}\n";
    const APPLIED: &str = r#""C:\Programs\Modlist Launcher\modlist-launcher.exe" --steam abc -- %command%"#;

    /// A fake Steam folder with one account whose Cyberpunk launch options
    /// were set by the launcher.
    fn fake_steam(current: &str) -> (tempfile::TempDir, SteamRoot, DataDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("Steam/userdata/123/config");
        std::fs::create_dir_all(&config).unwrap();
        let file = config.join("localconfig.vdf");
        std::fs::write(&file, steam::set_launch_options(BASE, 1091500, current).unwrap()).unwrap();
        let data = DataDir::new(tmp.path().join("data"));
        let steam = SteamRoot::new(tmp.path().join("Steam"));
        (tmp, steam, data, file)
    }

    fn hook() -> SteamHook {
        SteamHook {
            account_id: 123,
            app_id: 1091500,
            previous_launch_options: "-skipStartScreen".into(),
            applied_launch_options: APPLIED.into(),
        }
    }

    fn backups(data: &DataDir) -> usize {
        std::fs::read_dir(data.root().join("steam-backups")).map(|d| d.count()).unwrap_or(0)
    }

    #[test]
    fn restores_previous_options_with_a_backup() {
        let (_t, steam, data, file) = fake_steam(APPLIED);
        let before = std::fs::read_to_string(&file).unwrap();
        assert_eq!(restore(&hook(), Some(&steam), &data, false, |_| {}).unwrap(), Restored::Restored);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), BASE);
        assert_eq!(backups(&data), 1);
        let backup = std::fs::read_dir(data.root().join("steam-backups")).unwrap().next().unwrap().unwrap().path();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), before);
    }

    #[test]
    fn leaves_options_changed_elsewhere() {
        let (_t, steam, data, file) = fake_steam("-windowed");
        let before = std::fs::read_to_string(&file).unwrap();
        assert_eq!(restore(&hook(), Some(&steam), &data, false, |_| {}).unwrap(), Restored::LeftAsIs);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        assert_eq!(backups(&data), 0);
    }

    #[test]
    fn refuses_while_steam_runs_and_reports_missing_pieces() {
        let (_t, steam, data, file) = fake_steam(APPLIED);
        let before = std::fs::read_to_string(&file).unwrap();
        assert!(matches!(restore(&hook(), Some(&steam), &data, true, |_| {}), Err(RestoreError::SteamRunning)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);

        let mut other = hook();
        other.account_id = 999;
        assert!(matches!(restore(&other, Some(&steam), &data, false, |_| {}), Err(RestoreError::Failed(_))));
        assert!(matches!(restore(&hook(), None, &data, false, |_| {}), Err(RestoreError::Failed(_))));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }
}
