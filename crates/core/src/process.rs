//! Decides whether a launch is safe given the processes already running.
//!
//! Mod Organizer 2 uses one machine-wide single-instance key. A second MO2
//! process forwards its request to whichever MO2 is already running, and a
//! portable shortcut matches any portable instance. Starting install B while
//! install A runs would therefore act inside A, so the launcher blocks it.

use std::path::{Path, PathBuf};

use crate::config::Installation;
use crate::mo2::MO2_EXE;
use crate::paths::{file_name_eq, same_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    pub pid: u32,
    /// `None` when the image path could not be read (for example an elevated
    /// process).
    pub path: Option<PathBuf>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Clear,
    /// This installation's MO2 is already open.
    SameInstanceRunning {
        pid: u32,
    },
    /// A different MO2 is open; launching now would act inside it.
    OtherInstanceRunning {
        pid: u32,
        path: Option<PathBuf>,
        name: Option<String>,
    },
}

pub fn mo2_processes(running: &[RunningProcess]) -> impl Iterator<Item = &RunningProcess> {
    running
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(MO2_EXE) || p.path.as_deref().is_some_and(|x| file_name_eq(x, MO2_EXE)))
}

pub fn gate(target_exe: &Path, running: &[RunningProcess], known: &[Installation]) -> Gate {
    let mo2: Vec<_> = mo2_processes(running).collect();
    if let Some(same) = mo2.iter().find(|p| p.path.as_deref().is_some_and(|x| same_path(x, target_exe))) {
        return Gate::SameInstanceRunning { pid: same.pid };
    }
    match mo2.first() {
        None => Gate::Clear,
        Some(other) => Gate::OtherInstanceRunning {
            pid: other.pid,
            path: other.path.clone(),
            name: other
                .path
                .as_deref()
                .and_then(|p| known.iter().find(|i| same_path(&i.exe_path, p)))
                .map(|i| i.name.clone()),
        },
    }
}

pub fn is_running(path: &Path, running: &[RunningProcess]) -> Option<u32> {
    running.iter().find(|p| p.path.as_deref().is_some_and(|x| same_path(x, path))).map(|p| p.pid)
}

pub fn describe_block(gate: &Gate) -> Option<String> {
    match gate {
        Gate::OtherInstanceRunning { name: Some(name), .. } => Some(format!(
            "Mod Organizer is already open for {name}. Close it first: MO2 only allows one instance at a time, and launching now would act inside the other modlist."
        )),
        Gate::OtherInstanceRunning { path: Some(path), .. } => Some(format!(
            "Another Mod Organizer is already open ({}). Close it first: MO2 only allows one instance at a time.",
            path.display()
        )),
        Gate::OtherInstanceRunning { .. } => Some(
            "Another Mod Organizer is already open. Close it first: MO2 only allows one instance at a time.".into(),
        ),
        _ => None,
    }
}

/// The line MO2's exit-wait dialog shows for each process it waits on, such as
/// `Cyberpunk2077.exe (32576)`.
///
/// Started with a shortcut, MO2 runs the program and then waits to exit,
/// showing "Mod Organizer is waiting on an application to close before
/// exiting." Matching the process line rather than that sentence keeps the
/// check independent of MO2's interface language, and the pid ties it to
/// this session's game.
pub fn wait_dialog_entry(game_exe_name: &str, game_pid: u32) -> String {
    format!("{game_exe_name} ({game_pid})")
}

/// Whether a window's visible texts are MO2's exit-wait dialog for this game.
pub fn is_mo2_wait_dialog<S: AsRef<str>>(texts: &[S], game_exe_name: &str, game_pid: u32) -> bool {
    let entry = wait_dialog_entry(game_exe_name, game_pid);
    texts.iter().any(|t| t.as_ref().lines().any(|line| line.trim().eq_ignore_ascii_case(&entry)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_mo2_wait_dialog() {
        // Texts recorded from MO2 2.5.2's dialog during a Steam Play launch.
        let dialog = [
            "ModOrganizer",
            "System Menu Bar",
            "Close",
            "Mod Organizer is waiting on an application to close before exiting.",
            "Cyberpunk2077.exe (32576)",
            "Exit Now",
            "Cancel",
        ];
        assert!(is_mo2_wait_dialog(&dialog, "Cyberpunk2077.exe", 32576));
        assert!(is_mo2_wait_dialog(&dialog, "cyberpunk2077.EXE", 32576));
        // A different game process, or a window without the entry, is left alone.
        assert!(!is_mo2_wait_dialog(&dialog, "Cyberpunk2077.exe", 1234));
        let other = ["Mod Organizer", "Problems detected", "Cyberpunk2077.exe (32576) crashed"];
        assert!(!is_mo2_wait_dialog(&other, "Cyberpunk2077.exe", 32576));
        assert!(!is_mo2_wait_dialog::<&str>(&[], "Cyberpunk2077.exe", 32576));
        // The entry may share a text element with the sentence, on its own line.
        let combined =
            ["Mod Organizer is waiting on an application to close before exiting.\r\nCyberpunk2077.exe (32576)"];
        assert!(is_mo2_wait_dialog(&combined, "Cyberpunk2077.exe", 32576));
    }

    fn proc(pid: u32, path: Option<&str>) -> RunningProcess {
        RunningProcess {
            pid,
            path: path.map(PathBuf::from),
            name: path.map(|p| p.rsplit('\\').next().unwrap().to_string()).unwrap_or_else(|| "ModOrganizer.exe".into()),
        }
    }

    fn known() -> Vec<Installation> {
        vec![
            Installation::new("Project Ultrapunk", PathBuf::from(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")),
            Installation::new(
                "Witcher 3 Ascended",
                PathBuf::from(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe"),
            ),
        ]
    }

    #[test]
    fn clear_when_no_mo2() {
        let running = [proc(1, Some(r"C:\Windows\explorer.exe")), proc(2, Some(r"C:\Steam\steam.exe"))];
        assert_eq!(gate(&known()[0].exe_path, &running, &known()), Gate::Clear);
    }

    #[test]
    fn same_instance_matches_case_and_slashes() {
        let running = [proc(7, Some(r"\\?\c:\modlists\projectultrapunk\MODORGANIZER.EXE"))];
        assert_eq!(gate(&known()[0].exe_path, &running, &known()), Gate::SameInstanceRunning { pid: 7 });
    }

    #[test]
    fn other_instance_blocks_with_name() {
        let running = [proc(9, Some(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe"))];
        let g = gate(&known()[1].exe_path, &running, &known());
        assert!(matches!(&g, Gate::OtherInstanceRunning { pid: 9, name: Some(n), .. } if n == "Project Ultrapunk"));
        assert!(describe_block(&g).unwrap().contains("Project Ultrapunk"));
    }

    #[test]
    fn unknown_or_unreadable_mo2_still_blocks() {
        let running = [proc(3, None)];
        let g = gate(&known()[0].exe_path, &running, &known());
        assert!(matches!(g, Gate::OtherInstanceRunning { pid: 3, path: None, name: None }));
        assert!(describe_block(&g).is_some());

        let running = [proc(4, Some(r"D:\Other\ModOrganizer.exe"))];
        assert!(describe_block(&gate(&known()[0].exe_path, &running, &known())).unwrap().contains(r"D:\Other"));
    }

    #[test]
    fn same_instance_wins_over_others() {
        let running = [
            proc(1, Some(r"D:\Other\ModOrganizer.exe")),
            proc(2, Some(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")),
        ];
        assert_eq!(gate(&known()[0].exe_path, &running, &known()), Gate::SameInstanceRunning { pid: 2 });
    }

    #[test]
    fn is_running_finds_game() {
        let running = [proc(5, Some(r"C:\Games\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe"))];
        assert_eq!(is_running(Path::new(r"c:/games/cyberpunk 2077/bin/x64/cyberpunk2077.exe"), &running), Some(5));
        assert_eq!(is_running(Path::new(r"C:\x.exe"), &running), None);
    }
}
