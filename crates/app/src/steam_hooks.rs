//! Writing to and restoring a Steam account's `localconfig.vdf`. Shared by the
//! Steam Play Button sheet, the Remove flow and `--restore-steam`.
//!
//! Every write needs Steam closed, saves a backup in the launcher's data
//! folder first, replaces the file atomically and verifies it by reading back.

use std::path::{Path, PathBuf};

use modlist_core::config::{Config, DataDir, SteamHook, timestamp};
use modlist_core::steam::{self, RestorePlan, SteamRoot};

/// A Steam game whose Play button starts Modlist Launcher although no
/// modlist records that connection, so nothing else would ever undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub account_id: u32,
    pub app_id: u32,
    /// The game's name when it is installed; otherwise unknown here, because
    /// the launcher does not go online to look it up.
    pub name: Option<String>,
    pub localconfig: PathBuf,
}

impl Orphan {
    pub fn label(&self) -> String {
        match &self.name {
            Some(name) => format!("{name} (Steam app {})", self.app_id),
            None => format!("Steam app {} (not installed)", self.app_id),
        }
    }
}

/// Every orphaned connection across this PC's Steam accounts.
pub fn find_orphans(steam: &SteamRoot, config: &Config) -> Vec<Orphan> {
    let recorded = |account: u32, app: u32| {
        config
            .installations
            .iter()
            .any(|i| i.steam_hook.as_ref().is_some_and(|h| h.account_id == account && h.app_id == app))
    };
    let installed = steam.installed_apps();
    let mut found = Vec::new();
    for account in steam.accounts() {
        let Ok(text) = std::fs::read_to_string(&account.localconfig) else { continue };
        let Ok(connections) = steam::launcher_connections(&text) else { continue };
        for (app_id, _) in connections {
            if !recorded(account.account_id, app_id) {
                found.push(Orphan {
                    account_id: account.account_id,
                    app_id,
                    name: installed.iter().find(|a| a.app_id == app_id).map(|a| a.name.clone()),
                    localconfig: account.localconfig.clone(),
                });
            }
        }
    }
    found
}

/// The launch options an orphaned app had before the launcher touched it,
/// from the launcher's own backups (newest first). Empty when no backup
/// knows it, which is also what a game that never had options looks like.
pub fn original_value(data: &DataDir, account_id: u32, app_id: u32) -> String {
    let mut backups: Vec<PathBuf> = std::fs::read_dir(data.root().join("steam-backups"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("localconfig-{account_id}-")))
        })
        .collect();
    backups.sort();
    backups
        .iter()
        .rev()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .find_map(|text| steam::original_launch_options(&text, app_id))
        .unwrap_or_default()
}

/// Puts an orphaned app's launch options back to what they were before.
pub fn restore_orphan(
    orphan: &Orphan,
    data: &DataDir,
    steam_running: bool,
    log: impl Fn(&str),
) -> Result<Restored, RestoreError> {
    if steam_running {
        return Err(RestoreError::SteamRunning);
    }
    let fail = RestoreError::Failed;
    let text = std::fs::read_to_string(&orphan.localconfig)
        .map_err(|e| fail(format!("Could not read Steam's settings: {e}")))?;
    let current = steam::get_launch_options(&text, orphan.app_id).map_err(|e| fail(e.to_string()))?;
    if steam::launcher_target(&current).is_none() {
        return Ok(Restored::LeftAsIs);
    }
    let original = original_value(data, orphan.account_id, orphan.app_id);
    let hook = SteamHook {
        account_id: orphan.account_id,
        app_id: orphan.app_id,
        previous_launch_options: original,
        applied_launch_options: current,
    };
    let saved = backup(data, orphan.account_id, &text).map_err(fail)?;
    let RestorePlan::Restore(updated) =
        steam::plan_restore(&text, hook.app_id, &hook.applied_launch_options, &hook.previous_launch_options)
            .map_err(|e| fail(e.to_string()))?
    else {
        return Ok(Restored::LeftAsIs);
    };
    write(&orphan.localconfig, &updated).map_err(|e| fail(format!("Could not write Steam's settings: {e}")))?;
    let check =
        std::fs::read_to_string(&orphan.localconfig).ok().and_then(|t| steam::get_launch_options(&t, hook.app_id).ok());
    if check.as_deref() != Some(hook.previous_launch_options.as_str()) {
        let _ = write(&orphan.localconfig, &text);
        return Err(fail("Steam's settings did not read back correctly, so the original file was put back.".into()));
    }
    log(&format!(
        "steam: restored orphaned launch options for app {} to {:?}; backup {}",
        hook.app_id,
        hook.previous_launch_options,
        saved.display()
    ));
    Ok(Restored::Restored)
}

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

    /// Steam settings where Fallout 4 (377160) is a recorded connection and
    /// the Creation Kit (1946160) still points here with no record.
    fn orphan_setup() -> (tempfile::TempDir, SteamRoot, DataDir, PathBuf, Config) {
        let (tmp, steam, data, file) = fake_steam(APPLIED);
        let text = std::fs::read_to_string(&file).unwrap();
        let text = steam::set_launch_options(&text, 377160, APPLIED).unwrap();
        let text = steam::set_launch_options(&text, 1946160, APPLIED).unwrap();
        std::fs::write(&file, text).unwrap();
        let mut inst =
            modlist_core::config::Installation::new("Life in the Ruins", PathBuf::from(r"C:\L\ModOrganizer.exe"));
        inst.steam_hook = Some(SteamHook {
            account_id: 123,
            app_id: 377160,
            previous_launch_options: String::new(),
            applied_launch_options: APPLIED.into(),
        });
        let mut other =
            modlist_core::config::Installation::new("Project Ultrapunk", PathBuf::from(r"C:\U\ModOrganizer.exe"));
        other.steam_hook = Some(hook());
        let config = Config { installations: vec![inst, other], ..Config::default() };
        (tmp, steam, data, file, config)
    }

    #[test]
    fn finds_only_unrecorded_connections() {
        let (_t, steam, _data, _file, config) = orphan_setup();
        let orphans = find_orphans(&steam, &config);
        assert_eq!(orphans.len(), 1, "{orphans:?}");
        assert_eq!(orphans[0].app_id, 1946160);
        assert_eq!(orphans[0].label(), "Steam app 1946160 (not installed)");
    }

    #[test]
    fn original_value_comes_from_the_newest_backup_that_predates_the_launcher() {
        let (_t, _steam, data, _file, _config) = orphan_setup();
        let dir = data.root().join("steam-backups");
        std::fs::create_dir_all(&dir).unwrap();
        // Oldest: the app had its own options. Newer: already pointing here.
        std::fs::write(
            dir.join("localconfig-123-100.vdf"),
            steam::set_launch_options(BASE, 1946160, "-novid").unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("localconfig-123-200.vdf"), steam::set_launch_options(BASE, 1946160, APPLIED).unwrap())
            .unwrap();
        // Another account's backup is never used.
        std::fs::write(
            dir.join("localconfig-999-300.vdf"),
            steam::set_launch_options(BASE, 1946160, "-wrong").unwrap(),
        )
        .unwrap();
        assert_eq!(original_value(&data, 123, 1946160), "-novid");
        assert_eq!(original_value(&data, 123, 42), "", "no backup knows it");
    }

    #[test]
    fn restores_an_orphan_and_keeps_recorded_connections() {
        let (_t, steam, data, file, config) = orphan_setup();
        let orphan = find_orphans(&steam, &config).remove(0);
        assert!(matches!(restore_orphan(&orphan, &data, true, |_| {}), Err(RestoreError::SteamRunning)));
        assert_eq!(restore_orphan(&orphan, &data, false, |_| {}).unwrap(), Restored::Restored);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(steam::get_launch_options(&text, 1946160).unwrap(), "");
        assert_eq!(steam::get_launch_options(&text, 377160).unwrap(), APPLIED, "recorded connection untouched");
        assert_eq!(steam::get_launch_options(&text, 1091500).unwrap(), APPLIED, "recorded connection untouched");
        assert!(find_orphans(&steam, &config).is_empty());
        assert_eq!(backups(&data), 1);
        // A second run finds nothing left to do.
        assert_eq!(restore_orphan(&orphan, &data, false, |_| {}).unwrap(), Restored::LeftAsIs);
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
