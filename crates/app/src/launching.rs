//! The launch decision shared by the window, shortcuts and Steam.

use std::process::Child;

use modlist_core::config::Installation;
use modlist_core::launch::{self, LaunchMode};
use modlist_core::process::{self, Gate, RunningProcess};
use modlist_core::validate::{Inspection, display_message};

use crate::platform;

pub enum Outcome {
    /// MO2 was started; watch it for an early exit.
    Started(Child),
    /// This installation's MO2 was already open and was brought forward.
    Focused,
}

/// Checks everything that can be checked up front, then starts MO2.
pub fn launch(
    inst: &Installation,
    inspection: &Inspection,
    mode: LaunchMode,
    running: &[RunningProcess],
    all: &[Installation],
) -> Result<Outcome, String> {
    if !inspection.can_open() || (mode == LaunchMode::Play && !inspection.can_play()) {
        let msg = inspection
            .first_error()
            .map(|e| display_message(e).to_string())
            .unwrap_or_else(|| "This installation could not be checked.".into());
        return Err(msg);
    }
    let gate = process::gate(&inst.exe_path, running, all);
    if let Some(msg) = process::describe_block(&gate) {
        return Err(msg);
    }
    let effective =
        if matches!(inst.play_action, modlist_core::config::PlayAction::OpenMo2) { LaunchMode::OpenMo2 } else { mode };
    if let (Gate::SameInstanceRunning { pid }, LaunchMode::OpenMo2) = (&gate, effective) {
        // A bare second MO2 would only show "already running"; bring the open one forward.
        return if platform::processes::focus_process_window(*pid) {
            Ok(Outcome::Focused)
        } else {
            Err("Mod Organizer is already open for this modlist, but its window could not be brought forward.".into())
        };
    }
    // For Play with this MO2 already open, MO2 forwards the shortcut to the
    // open instance, which is the same installation.
    let spec = launch::build(inst, mode);
    platform::launch::spawn(&spec)
        .map(Outcome::Started)
        .map_err(|e| format!("Could not start {}: {e}", spec.program.display()))
}

/// Describes an MO2 that quit within a few seconds of starting.
pub fn early_exit_message(status: std::process::ExitStatus, inst: &Installation) -> Option<String> {
    if status.success() {
        return None;
    }
    let code = status.code().map_or_else(|| "unknown".to_string(), |c| c.to_string());
    Some(format!(
        "Mod Organizer for {} closed right after starting (exit code {code}). Its own log is in {}.",
        inst.name,
        inst.working_directory.join("logs").display()
    ))
}
