//! Launches that come from a shortcut or from Steam and show no main window.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use modlist_core::config::{Installation, PlayAction};
use modlist_core::launch::LaunchMode;
use modlist_core::process::{self, GameState, Gate};
use modlist_core::validate::{self, display_message};
use slint::ComponentHandle;

use crate::launching::{self, Outcome};
use crate::state::{AppState, running_mo2};
use crate::{MessageWindow, PromptWindow, SteamChooser, WaitWindow, platform};

pub fn show_message(heading: &str, message: &str) {
    let Ok(w) = MessageWindow::new() else { return };
    w.set_heading(heading.into());
    w.set_message(message.into());
    let weak = w.as_weak();
    w.on_ok(move || {
        if let Some(w) = weak.upgrade() {
            let _ = w.hide();
        }
    });
    let _ = w.run();
}

/// Shows a two-choice prompt; true when the primary choice was picked.
pub fn prompt(heading: &str, message: &str, primary: &str, secondary: &str) -> bool {
    let Ok(w) = PromptWindow::new() else { return false };
    w.set_heading(heading.into());
    w.set_message(message.into());
    w.set_primary_label(primary.into());
    w.set_secondary_label(secondary.into());
    let picked = std::rc::Rc::new(std::cell::Cell::new(false));
    for (choice, setter) in [
        (true, PromptWindow::on_primary as fn(&PromptWindow, _)),
        (false, PromptWindow::on_secondary as fn(&PromptWindow, _)),
    ] {
        let (picked, weak) = (picked.clone(), w.as_weak());
        setter(&w, move || {
            picked.set(choice);
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
    }
    let _ = w.run();
    picked.get()
}

/// `--restore-steam`, run by the uninstaller: puts back every game's previous
/// Steam launch options so Steam's Play button does not point at a removed
/// launcher. Silent when there is nothing to do or everything worked.
pub fn restore_steam() -> i32 {
    let (mut state, _) = AppState::load();
    let hooked: Vec<_> = state
        .config
        .installations
        .iter()
        .filter_map(|i| i.steam_hook.clone().map(|h| (i.id, i.name.clone(), h)))
        .collect();
    if hooked.is_empty() {
        return 0;
    }
    let names = hooked.iter().map(|(_, n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
    let mut problems = Vec::new();
    for (id, name, hook) in &hooked {
        let result = loop {
            match crate::steam_hooks::restore(
                hook,
                state.steam.as_ref(),
                state.data(),
                platform::steam::is_running(),
                |l| state.log(l),
            ) {
                Err(crate::steam_hooks::RestoreError::SteamRunning) => {
                    let retry = prompt(
                        "Exit Steam to finish",
                        &format!(
                            "Steam's Play button still starts {names} through Modlist Launcher. Exit Steam completely (Steam menu > Exit), then choose Retry to put back the previous launch options. If you skip, change them yourself in Steam: right-click the game > Properties > General > Launch Options."
                        ),
                        "Retry",
                        "Skip",
                    );
                    if !retry {
                        break Err("skipped while Steam was running".to_string());
                    }
                }
                other => break other.map_err(|e| e.to_string()),
            }
        };
        match result {
            Ok(_) => {
                if let Some(inst) = state.config.find_mut(*id) {
                    inst.steam_hook = None;
                }
            }
            Err(e) => {
                state.log(&format!("restore-steam: {name}: {e}"));
                problems.push(format!("{name}: {e}"));
                if e.starts_with("skipped") {
                    break;
                }
            }
        }
    }
    let _ = state.save();
    if problems.is_empty() {
        return 0;
    }
    show_message(
        "Steam launch options not restored",
        &format!(
            "{}\n\nIn Steam, right-click the game > Properties > General > Launch Options, and clear the Modlist Launcher entry.",
            problems.join("\n")
        ),
    );
    1
}

fn find<'a>(state: &'a AppState, target: &str) -> Result<&'a Installation, String> {
    state.config.resolve(target).ok_or_else(|| {
        format!("\"{target}\" is not in Modlist Launcher anymore. Open the launcher to add it again or update this shortcut.")
    })
}

enum Settled {
    /// The game is not running (any more); go ahead.
    Clear,
    /// Nothing more to do; exit with this code.
    Done(i32),
}

/// Before starting Play: if the game is already up, bring it forward; if it is
/// still closing, wait for it (with a Cancel button) and then carry on.
fn settle_game(inst: &Installation, inspection: &validate::Inspection) -> Settled {
    let Some(binary) = launching::play_binary(inst, inspection) else { return Settled::Clear };
    let name = launching::game_name(inspection, &binary);
    match launching::game_state(&binary, |_| None) {
        GameState::NotRunning => Settled::Clear,
        GameState::Running { pid } => {
            platform::processes::focus_process_window(pid);
            Settled::Done(0)
        }
        busy => {
            let Some(pid) = busy.pid() else { return Settled::Clear };
            match wait_while_busy(pid, &name) {
                Wait::Exited => Settled::Clear,
                Wait::WindowAppeared => {
                    platform::processes::focus_process_window(pid);
                    Settled::Done(0)
                }
                Wait::Cancelled => Settled::Done(0),
                Wait::TimedOut => {
                    show_message(
                        &format!("{name} is still running"),
                        &format!(
                            "{name} did not close within three minutes. Close it, or end it in Task Manager, then try again."
                        ),
                    );
                    Settled::Done(1)
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    Exited,
    WindowAppeared,
    Cancelled,
    TimedOut,
}

/// Shows "still closing" until the game process exits, shows a window (it was
/// starting, not closing), or the user cancels.
fn wait_while_busy(pid: u32, name: &str) -> Wait {
    let Ok(w) = WaitWindow::new() else { return Wait::Cancelled };
    w.set_heading(format!("{name} is still closing").into());
    w.set_message("Modlist Launcher will start it with mods as soon as the previous run has finished.".into());
    let outcome = std::rc::Rc::new(std::cell::Cell::new(Wait::Cancelled));
    {
        let (outcome, weak) = (outcome.clone(), w.as_weak());
        w.on_cancel(move || {
            outcome.set(Wait::Cancelled);
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
    }
    let started = Instant::now();
    let timer = slint::Timer::default();
    {
        let (outcome, weak) = (outcome.clone(), w.as_weak());
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(400), move || {
            let result = if platform::processes::wait_for_exit(pid, Some(Duration::ZERO)) {
                Some(Wait::Exited)
            } else if !platform::other_windows::visible_top_level(pid).is_empty() {
                Some(Wait::WindowAppeared)
            } else if started.elapsed() > Duration::from_secs(180) {
                Some(Wait::TimedOut)
            } else {
                None
            };
            if let Some(result) = result {
                outcome.set(result);
                if let Some(w) = weak.upgrade() {
                    let _ = w.hide();
                }
            }
        });
    }
    let _ = w.run();
    outcome.get()
}

/// `--launch` / `--open` from a per-modlist shortcut.
pub fn launch(target: &str, mode: LaunchMode) -> i32 {
    let (state, _) = AppState::load();
    let inst = match find(&state, target) {
        Ok(i) => i,
        Err(e) => {
            show_message("Modlist not found", &e);
            return 1;
        }
    };
    let inspection = validate::inspect(inst);
    if mode == LaunchMode::Play
        && let Settled::Done(code) = settle_game(inst, &inspection)
    {
        return code;
    }
    let running = running_mo2();
    match launching::launch(inst, &inspection, mode, &running, &state.config.installations, GameState::NotRunning) {
        Ok(Outcome::Focused) => 0,
        Ok(Outcome::Started(mut child)) => {
            state.log(&format!("shortcut launch: {} ({mode:?})", inst.name));
            if let Some(msg) = platform::launch::early_exit(&mut child, Duration::from_secs(5))
                .and_then(|s| launching::early_exit_message(s, inst))
            {
                show_message("Mod Organizer did not start", &msg);
                return 1;
            }
            0
        }
        Err(e) => {
            state.log(&format!("shortcut launch blocked: {} - {e}", inst.name));
            show_message(&format!("Can't start {}", inst.name), &e);
            1
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Modded,
    Vanilla,
    Cancel,
}

fn choose(name: &str, reason: &str, can_mod: bool, can_vanilla: bool, hero: Option<slint::Image>) -> Choice {
    let Ok(w) = SteamChooser::new() else { return Choice::Cancel };
    w.set_name(name.into());
    w.set_reason(reason.into());
    w.set_can_mod(can_mod);
    w.set_can_vanilla(can_vanilla);
    w.set_has_hero(hero.is_some());
    w.set_hero(hero.unwrap_or_default());
    let choice = std::rc::Rc::new(std::cell::Cell::new(Choice::Cancel));
    for (pick, setter) in [
        (Choice::Modded, SteamChooser::on_modded as fn(&SteamChooser, _)),
        (Choice::Vanilla, SteamChooser::on_vanilla as fn(&SteamChooser, _)),
        (Choice::Cancel, SteamChooser::on_cancel as fn(&SteamChooser, _)),
    ] {
        let (choice, weak) = (choice.clone(), w.as_weak());
        setter(&w, move || {
            choice.set(pick);
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
    }
    let _ = w.run();
    choice.get()
}

/// `--steam <id> -- %command%`, started by Steam's Play button. Stays alive
/// until the game exits so Steam tracks the session and syncs saves after.
pub fn steam(target: &str, original: &[String]) -> i32 {
    let (state, _) = AppState::load();
    let can_vanilla = original.first().is_some_and(|p| Path::new(p).is_file());
    let inst = match find(&state, target) {
        Ok(i) => i,
        Err(e) => {
            return match choose("this game", &e, false, can_vanilla, None) {
                Choice::Vanilla => run_original(original),
                _ => 1,
            };
        }
    };
    let inspection = validate::inspect(inst);
    if let Settled::Done(code) = settle_game(inst, &inspection) {
        return code;
    }
    let running = running_mo2();
    let gate = process::gate(&inst.exe_path, &running, &state.config.installations);

    let mut reason = String::new();
    if !inspection.can_play() {
        reason = inspection.first_error().map(|e| display_message(e).to_string()).unwrap_or_default();
    } else if let Some(block) = process::describe_block(&gate) {
        reason = block;
    }
    let can_mod = reason.is_empty();
    let choice = if !can_mod || platform::shift_held() {
        let hero = inst
            .steam_app_id
            .zip(state.steam.as_ref())
            .and_then(|(app, s)| s.library_image(app, "library_hero.jpg"))
            .and_then(|p| slint::Image::load_from_path(&p).ok());
        choose(&inst.name, &reason, can_mod, can_vanilla, hero)
    } else {
        Choice::Modded
    };
    state.log(&format!(
        "steam launch: {} choice={}",
        inst.name,
        match choice {
            Choice::Modded => "modded",
            Choice::Vanilla => "vanilla",
            Choice::Cancel => "cancel",
        }
    ));

    match choice {
        Choice::Cancel => 0,
        Choice::Vanilla => run_original(original),
        Choice::Modded => {
            let child = match launching::launch(
                inst,
                &inspection,
                LaunchMode::Play,
                &running,
                &state.config.installations,
                GameState::NotRunning,
            ) {
                Ok(Outcome::Started(c)) => Some(c),
                Ok(Outcome::Focused) => None,
                Err(e) => {
                    show_message(&format!("Can't start {}", inst.name), &e);
                    return 1;
                }
            };
            wait_for_session(inst, &inspection, child, matches!(gate, Gate::SameInstanceRunning { .. }));
            0
        }
    }
}

fn run_original(original: &[String]) -> i32 {
    let Some(program) = original.first() else { return 1 };
    let program = PathBuf::from(program);
    let cwd = program.parent().map(Path::to_path_buf).unwrap_or_default();
    let spec = modlist_core::launch::LaunchSpec { program, args: original[1..].to_vec(), working_directory: cwd };
    match platform::launch::spawn(&spec) {
        Ok(mut child) => {
            let _ = child.wait();
            0
        }
        Err(e) => {
            show_message("Could not start the game", &e.to_string());
            1
        }
    }
}

/// Waits for the game that MO2 starts, or for MO2 itself when the Play
/// action only opens MO2.
fn wait_for_session(
    inst: &Installation,
    inspection: &validate::Inspection,
    child: Option<std::process::Child>,
    mo2_was_open: bool,
) {
    let game = match &inst.play_action {
        PlayAction::Executable { title } => {
            inspection.install.as_ref().and_then(|i| i.executable(title)).map(|e| e.binary.clone())
        }
        PlayAction::OpenMo2 => None,
    };
    let Some(game) = game else {
        if let Some(mut c) = child {
            let _ = c.wait();
        }
        return;
    };
    let name = game.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let started = Instant::now();
    let mut child = child;
    // Give MO2 up to five minutes to build its virtual file system and start the game.
    while started.elapsed() < Duration::from_secs(300) {
        let procs = platform::processes::running(|n| n.eq_ignore_ascii_case(&name));
        if let Some(pid) = process::is_running(&game, &procs) {
            // Only an MO2 this launch started shows the exit-wait dialog; an
            // already-open MO2 takes the forwarded shortcut in its main window.
            let mo2_pid = if mo2_was_open { None } else { child.as_ref().map(std::process::Child::id) };
            wait_for_game(pid, &name, mo2_pid);
            return;
        }
        if !mo2_was_open
            && let Some(c) = child.as_mut()
            && let Ok(Some(status)) = c.try_wait()
        {
            if let Some(msg) = launching::early_exit_message(status, inst) {
                show_message("Mod Organizer did not start", &msg);
            }
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Waits for the game to exit. Meanwhile, MO2 (started with a shortcut) shows
/// "waiting on an application to close before exiting" over the loading game;
/// that dialog is minimized once when it appears. MO2 still closes itself
/// when the game exits, and nothing else about MO2 is touched.
fn wait_for_game(game_pid: u32, game_exe_name: &str, mo2_pid: Option<u32>) {
    let started = Instant::now();
    let mut watch_dialog = mo2_pid.is_some();
    loop {
        if platform::processes::wait_for_exit(game_pid, Some(Duration::from_millis(500))) {
            return;
        }
        if let (true, Some(mo2)) = (watch_dialog, mo2_pid) {
            // Stop after it is minimized once, or after three minutes without it.
            watch_dialog = !platform::other_windows::minimize_mo2_wait_dialog(mo2, game_exe_name, game_pid)
                && started.elapsed() < Duration::from_secs(180);
        }
    }
}
