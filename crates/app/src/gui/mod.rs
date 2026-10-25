//! The main window: wires UI callbacks to state changes.
//!
//! Borrow rule: never hold the controller borrow across a native dialog. Its
//! nested message loop can run timers that borrow the controller again.

mod actions;
mod edit;
mod scan;
mod shortcuts;
mod steam;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use modlist_core::config::Appearance;
use modlist_core::launch::LaunchMode;
use modlist_core::process::RunningProcess;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use uuid::Uuid;
use windows::Win32::Foundation::HWND;

use crate::art::ArtCache;
use crate::cards::{self, CardInput};
use crate::launching::{self, Outcome};
use crate::state::{AppState, parse_id, running_mo2};
use crate::{AppWindow, InstallCard, Ui};

pub struct Controller {
    pub ui: slint::Weak<AppWindow>,
    pub state: AppState,
    pub art: ArtCache,
    pub running: Vec<RunningProcess>,
    pub starting: HashSet<Uuid>,
    pub cards: Rc<VecModel<InstallCard>>,
    pub profile_models: HashMap<Uuid, (Vec<String>, ModelRc<SharedString>)>,
    pub edit: edit::EditDraft,
    pub scan: scan::ScanDraft,
    pub steam: steam::SteamDraft,
    pub confirm: Option<actions::Pending>,
    pub watches: Vec<Watch>,
    pub games: Games,
    pub orphans: Vec<crate::steam_hooks::Orphan>,
    pub shortcuts: shortcuts::ShortcutIndex,
    /// Since when each running MO2 has shown no window, to spot a stuck one.
    pub mo2_windows: modlist_core::process::WindowlessTracker,
}

/// What each installation's game is doing, with enough history to tell a
/// game that is starting from one that is closing.
#[derive(Default)]
pub struct Games {
    pub states: HashMap<Uuid, modlist_core::process::GameState>,
    /// Game processes seen at all, and those seen with a window.
    observed: HashSet<u32>,
    windowed: HashSet<u32>,
    /// When this launcher last started each installation's Play action.
    launched: HashMap<Uuid, std::time::Instant>,
}

impl Games {
    pub fn update(
        &mut self,
        installs: &[modlist_core::config::Installation],
        inspections: &HashMap<Uuid, modlist_core::validate::Inspection>,
    ) {
        use modlist_core::process::GameState;
        self.states.clear();
        for inst in installs {
            let Some(binary) = inspections.get(&inst.id).and_then(|i| launching::play_binary(inst, i)) else {
                continue;
            };
            let recently_launched = self.launched.get(&inst.id).is_some_and(|t| t.elapsed() < Duration::from_secs(180));
            let state = launching::game_state(&binary, |pid| {
                if self.windowed.contains(&pid) {
                    Some(true)
                } else if self.observed.contains(&pid) || recently_launched {
                    Some(false)
                } else {
                    None
                }
            });
            if let Some(pid) = state.pid() {
                self.observed.insert(pid);
                if matches!(state, GameState::Running { .. }) {
                    self.windowed.insert(pid);
                }
            }
            self.states.insert(inst.id, state);
        }
    }

    pub fn state(&self, id: Uuid) -> modlist_core::process::GameState {
        self.states.get(&id).copied().unwrap_or(modlist_core::process::GameState::NotRunning)
    }

    pub fn launched(&mut self, id: Uuid) {
        self.launched.insert(id, std::time::Instant::now());
    }
}

pub struct Watch {
    id: Uuid,
    inst: modlist_core::config::Installation,
    rx: std::sync::mpsc::Receiver<Option<std::process::ExitStatus>>,
}

pub type Shared = Rc<RefCell<Controller>>;

pub fn run() -> i32 {
    let ui = match AppWindow::new() {
        Ok(ui) => ui,
        Err(e) => {
            crate::headless::show_message("Modlist Launcher could not start", &e.to_string());
            return 1;
        }
    };
    let (state, warnings) = AppState::load();
    let cards = Rc::new(VecModel::default());
    ui.global::<Ui>().set_cards(cards.clone().into());
    ui.global::<Ui>().set_version(env!("CARGO_PKG_VERSION").into());
    ui.global::<Ui>().set_data_folder(state.data().root().display().to_string().into());
    ui.global::<Ui>().set_appearance(match state.config.settings.appearance {
        Appearance::System => 0,
        Appearance::Light => 1,
        Appearance::Dark => 2,
    });
    ui.global::<Ui>().set_close_after_launch(state.config.settings.close_after_launch);
    if let Some(w) = warnings.first() {
        ui.global::<Ui>().set_banner_error(true);
        ui.global::<Ui>().set_banner_text(w.into());
    }

    let c: Shared = Rc::new(RefCell::new(Controller {
        ui: ui.as_weak(),
        state,
        art: ArtCache::default(),
        running: running_mo2(),
        starting: HashSet::new(),
        cards,
        profile_models: HashMap::new(),
        edit: Default::default(),
        scan: Default::default(),
        steam: Default::default(),
        confirm: None,
        watches: Vec::new(),
        games: Games::default(),
        orphans: Vec::new(),
        shortcuts: Default::default(),
        mo2_windows: Default::default(),
    }));
    c.borrow_mut().shortcuts.refresh(true);
    c.borrow_mut().refresh_cards();
    {
        let mut ctl = c.borrow_mut();
        ctl.refresh_orphans();
        if !ctl.orphans.is_empty() && ui.global::<Ui>().get_banner_text().is_empty() {
            let n = ctl.orphans.len();
            ctl.banner(
                &format!(
                    "{} still start{} Modlist Launcher from Steam's Play button without a modlist. Open Settings to restore {}.",
                    if n == 1 { ctl.orphans[0].label() } else { format!("{n} Steam games") },
                    if n == 1 { "s" } else { "" },
                    if n == 1 { "it" } else { "them" },
                ),
                true,
            );
        }
    }
    wire(&ui, &c);
    edit::wire(&ui, &c);
    scan::wire(&ui, &c);
    scan::startup_scan(&c);
    steam::wire(&ui, &c);
    actions::wire(&ui, &c);

    let timer = slint::Timer::default();
    {
        let c = c.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(3), move || {
            if let Ok(mut c) = c.try_borrow_mut() {
                c.running = running_mo2();
                c.observe_mo2_windows();
                c.state.inspect_all();
                let c = &mut *c;
                c.games.update(&c.state.config.installations, &c.state.inspections);
                c.shortcuts.refresh(false);
                c.refresh_cards();
            }
        });
    }
    let watcher = slint::Timer::default();
    {
        let c = c.clone();
        watcher.start(slint::TimerMode::Repeated, Duration::from_millis(250), move || {
            if let Ok(mut c) = c.try_borrow_mut() {
                c.check_watches();
            }
        });
    }
    let _repaint = repaint_after_restore(&ui);
    match ui.run() {
        Ok(()) => 0,
        Err(e) => {
            crate::headless::show_message("Modlist Launcher stopped", &e.to_string());
            1
        }
    }
}

/// Forces full repaints for a moment after the window is restored from
/// minimized. Slint's software renderer otherwise repaints only changed items
/// and leaves the rest of the restored window unpainted. Restoring passes
/// through several intermediate sizes, so a few frames are forced.
fn repaint_after_restore(ui: &AppWindow) -> slint::Timer {
    const FORCED_FRAMES: u8 = 4;
    let weak = ui.as_weak();
    let mut was_minimized = false;
    let mut pending = 0u8;
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return };
        let minimized = window_hwnd(&ui).is_some_and(crate::platform::other_windows::is_minimized);
        if was_minimized && !minimized {
            pending = FORCED_FRAMES;
        }
        was_minimized = minimized;
        if pending > 0 && !minimized {
            pending -= 1;
            ui.set_repaint_flip(!ui.get_repaint_flip());
            ui.window().request_redraw();
        }
    });
    timer
}

fn window_hwnd(ui: &AppWindow) -> Option<HWND> {
    let handle = ui.window().window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

fn wire(ui: &AppWindow, c: &Shared) {
    let g = ui.global::<Ui>();
    {
        let c = c.clone();
        g.on_play(move |id| launch(&c, &id, LaunchMode::Play));
    }
    {
        let c = c.clone();
        g.on_open_mo2(move |id| launch(&c, &id, LaunchMode::OpenMo2));
    }
    {
        let c = c.clone();
        g.on_profile_selected(move |id, index| c.borrow_mut().select_profile(&id, index));
    }
    {
        let c = c.clone();
        g.on_close_sheet(move || c.borrow().close_sheet());
    }
    {
        let c = c.clone();
        g.on_settings_changed(move || c.borrow_mut().settings_changed());
    }
    {
        let c = c.clone();
        g.on_open_settings(move || {
            let mut ctl = c.borrow_mut();
            ctl.refresh_orphans();
            if let Some(ui) = ctl.ui() {
                ui.global::<Ui>().set_orphans_status("".into());
                ui.global::<Ui>().set_sheet("settings".into());
            }
        });
    }
    {
        let c = c.clone();
        g.on_restore_orphan(move |i| {
            let mut ctl = c.borrow_mut();
            let result = ctl.restore_orphan(i.max(0) as usize);
            ctl.refresh_orphans();
            if let Some(ui) = ctl.ui() {
                let g = ui.global::<Ui>();
                g.set_orphans_status_error(result.is_err());
                g.set_orphans_status(result.unwrap_or_else(|e| e).into());
                if ctl.orphans.is_empty()
                    && g.get_banner_error()
                    && g.get_banner_text().contains("Open Settings to restore")
                {
                    g.set_banner_text("".into());
                    ctl.show_pending_notice();
                }
            }
        });
    }
    {
        let c = c.clone();
        g.on_open_data_folder(move || {
            let c = c.borrow();
            let root = c.state.data().root().to_path_buf();
            let _ = std::fs::create_dir_all(&root);
            if let Err(e) = crate::platform::shell::open_in_explorer(&root) {
                c.banner(&format!("Could not open {}: {e}", root.display()), true);
            }
        });
    }
}

fn launch(c: &Shared, id: &str, mode: LaunchMode) {
    let mut ctl = c.borrow_mut();
    let Some(uuid) = parse_id(id) else { return };
    ctl.running = running_mo2();
    ctl.observe_mo2_windows();
    ctl.state.inspect(uuid);
    let Some((inst, Some(_))) = ctl.state.get(uuid) else { return };
    let inst = inst.clone();
    let ctl = &mut *ctl;
    // A stuck MO2 (running, no window) cannot be shown or reused; offer to end it.
    if let Some(pid) = modlist_core::process::is_running(&inst.exe_path, &ctl.running)
        && let Some(stuck) = ctl.mo2_windows.stuck_for(pid, std::time::Instant::now())
    {
        ctl.ask_end_stuck_mo2(uuid, pid, stuck);
        return;
    }
    ctl.games.update(&ctl.state.config.installations, &ctl.state.inspections);
    let Some(insp) = ctl.state.inspections.get(&uuid) else { return };
    let result =
        launching::launch(&inst, insp, mode, &ctl.running, &ctl.state.config.installations, ctl.games.state(uuid));
    if result.is_ok() && mode == LaunchMode::Play {
        ctl.games.launched(uuid);
    }
    match result {
        Ok(Outcome::Focused) => {}
        Ok(Outcome::Started(mut child)) => {
            ctl.state.log(&format!("launch: {} ({mode:?})", inst.name));
            ctl.starting.insert(uuid);
            ctl.refresh_cards();
            // Watch for an early exit off the UI thread so the window stays responsive.
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::platform::launch::early_exit(&mut child, Duration::from_secs(5)));
            });
            ctl.watches.push(Watch { id: uuid, inst, rx });
        }
        Err(e) => {
            ctl.state.log(&format!("launch blocked: {} - {e}", inst.name));
            ctl.banner(&e, true);
        }
    }
}

impl Controller {
    pub fn ui(&self) -> Option<AppWindow> {
        self.ui.upgrade()
    }

    pub fn hwnd(&self) -> Option<HWND> {
        window_hwnd(&self.ui()?)
    }

    /// Notes which running MO2 processes have a visible window.
    pub fn observe_mo2_windows(&mut self) {
        let now = std::time::Instant::now();
        for p in &self.running {
            let has_window = !crate::platform::other_windows::visible_top_level(p.pid).is_empty();
            self.mo2_windows.observe(p.pid, has_window, now);
        }
        let alive: Vec<u32> = self.running.iter().map(|p| p.pid).collect();
        self.mo2_windows.retain(|pid| alive.contains(&pid));
    }

    /// Re-reads which Steam games still point here without a modlist.
    pub fn refresh_orphans(&mut self) {
        self.orphans = self
            .state
            .steam
            .as_ref()
            .map(|s| crate::steam_hooks::find_orphans(s, &self.state.config))
            .unwrap_or_default();
        if let Some(ui) = self.ui() {
            let labels: Vec<SharedString> = self.orphans.iter().map(|o| o.label().into()).collect();
            ui.global::<Ui>().set_steam_orphans(Rc::new(VecModel::from(labels)).into());
        }
    }

    fn restore_orphan(&mut self, index: usize) -> Result<String, String> {
        let orphan = self.orphans.get(index).cloned().ok_or("That Steam game was already restored.")?;
        let outcome =
            crate::steam_hooks::restore_orphan(&orphan, self.state.data(), crate::platform::steam::is_running(), |l| {
                self.state.log(l)
            })
            .map_err(|e| e.to_string())?;
        Ok(match outcome {
            crate::steam_hooks::Restored::Restored => format!("Restored {}'s launch options.", orphan.label()),
            crate::steam_hooks::Restored::LeftAsIs => {
                format!(
                    "{}'s launch options were changed outside the launcher, so they were left as they are.",
                    orphan.label()
                )
            }
        })
    }

    pub fn banner(&self, text: &str, error: bool) {
        if let Some(ui) = self.ui() {
            ui.global::<Ui>().set_banner_action("".into());
            ui.global::<Ui>().set_banner_error(error);
            ui.global::<Ui>().set_banner_text(text.into());
        }
    }

    pub fn close_sheet(&self) {
        if let Some(ui) = self.ui() {
            ui.global::<Ui>().set_sheet("".into());
        }
    }

    pub fn open_sheet(&self, name: &str) {
        if let Some(ui) = self.ui() {
            ui.global::<Ui>().set_sheet(name.into());
        }
    }

    pub fn save_or_report(&mut self) -> bool {
        match self.state.save() {
            Ok(()) => true,
            Err(e) => {
                self.banner(&e, true);
                false
            }
        }
    }

    fn profiles_model(&mut self, id: Uuid) -> ModelRc<SharedString> {
        let options = cards::profile_options(self.state.inspections.get(&id));
        if let Some((cached, model)) = self.profile_models.get(&id)
            && *cached == options
        {
            return model.clone();
        }
        let model: ModelRc<SharedString> =
            Rc::new(VecModel::from(options.iter().map(SharedString::from).collect::<Vec<_>>())).into();
        self.profile_models.insert(id, (options, model.clone()));
        model
    }

    /// Updates cards in place so keyboard focus survives the periodic refresh.
    pub fn refresh_cards(&mut self) {
        let now = std::time::Instant::now();
        let ids: Vec<Uuid> = self.state.config.installations.iter().map(|i| i.id).collect();
        let mut fresh = Vec::with_capacity(ids.len());
        for id in &ids {
            let profiles = self.profiles_model(*id);
            let Some(inst) = self.state.config.find(*id) else { continue };
            let inspection = self.state.inspections.get(id);
            let art = self.art.get(
                inst,
                inspection.and_then(|i| i.install.as_ref()),
                self.state.steam.as_ref(),
                self.state.data(),
            );
            fresh.push(cards::build(CardInput {
                inst,
                inspection,
                art,
                running: &self.running,
                all: &self.state.config.installations,
                starting: self.starting.contains(id),
                game: self.games.state(*id),
                mo2_stuck: modlist_core::process::is_running(&inst.exe_path, &self.running)
                    .and_then(|pid| self.mo2_windows.stuck_for(pid, now)),
                desktop_shortcut: self.shortcuts.on_desktop(&self.state.config, inst),
                start_menu_shortcut: self.shortcuts.in_start_menu(&self.state.config, inst),
                profiles,
            }));
        }
        let same_ids =
            self.cards.row_count() == fresh.len() && self.cards.iter().zip(&fresh).all(|(a, b)| a.id == b.id);
        if !same_ids {
            self.cards.set_vec(fresh);
            return;
        }
        for (i, card) in fresh.into_iter().enumerate() {
            if self.cards.row_data(i).as_ref() != Some(&card) {
                self.cards.set_row_data(i, card);
            }
        }
    }

    fn check_watches(&mut self) {
        if self.watches.is_empty() {
            return;
        }
        let mut finished = Vec::new();
        self.watches.retain(|w| match w.rx.try_recv() {
            Ok(status) => {
                finished.push((w.id, w.inst.clone(), status));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                finished.push((w.id, w.inst.clone(), None));
                false
            }
        });
        if finished.is_empty() {
            return;
        }
        self.running = running_mo2();
        for (id, inst, status) in finished {
            self.starting.remove(&id);
            match status.and_then(|s| launching::early_exit_message(s, &inst)) {
                Some(msg) => {
                    self.state.log(&msg);
                    self.banner(&msg, true);
                }
                None if self.state.config.settings.close_after_launch => {
                    let _ = slint::quit_event_loop();
                }
                None => {}
            }
        }
        self.refresh_cards();
    }

    fn select_profile(&mut self, id: &str, index: i32) {
        let Some(uuid) = parse_id(id) else { return };
        let profiles = self
            .state
            .inspections
            .get(&uuid)
            .and_then(|i| i.install.as_ref())
            .map(|i| i.profiles.clone())
            .unwrap_or_default();
        let choice = (index > 0).then(|| profiles.get(index as usize - 1).cloned()).flatten();
        if let Some(inst) = self.state.config.find_mut(uuid) {
            if inst.profile == choice {
                return;
            }
            inst.profile = choice;
        }
        self.save_or_report();
        self.state.inspect(uuid);
        self.refresh_cards();
    }

    fn settings_changed(&mut self) {
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<Ui>();
        self.state.config.settings.appearance = match g.get_appearance() {
            1 => Appearance::Light,
            2 => Appearance::Dark,
            _ => Appearance::System,
        };
        self.state.config.settings.close_after_launch = g.get_close_after_launch();
        self.save_or_report();
    }
}
