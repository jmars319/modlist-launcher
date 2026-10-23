//! Per-card action sheet: shortcuts, reordering, folder, removal.

use std::path::PathBuf;

use modlist_core::config::{Artwork, PlayAction};
use slint::ComponentHandle;
use uuid::Uuid;

use super::{Controller, Shared};
use crate::platform::shell::{self, Shortcut, ShortcutError, ShortcutPlace};
use crate::state::{current_exe, parse_id};
use crate::{AppWindow, Ui};

pub enum Pending {
    Remove(Uuid),
    /// Restoring Steam's launch options failed; remove without restoring.
    RemoveSkippingSteam(Uuid),
    ReplaceShortcut {
        id: Option<Uuid>,
        place: ShortcutPlace,
    },
}

pub fn wire(ui: &AppWindow, c: &Shared) {
    let g = ui.global::<Ui>();
    {
        let c = c.clone();
        g.on_show_actions(move |id| {
            if let Some(id) = parse_id(&id) {
                c.borrow().show_actions(id);
            }
        });
    }
    {
        let c = c.clone();
        g.on_move_by(move |id, delta| {
            let mut ctl = c.borrow_mut();
            let Some(id) = parse_id(&id) else { return };
            if ctl.state.config.move_by(id, delta as isize) {
                ctl.save_or_report();
                ctl.refresh_cards();
            }
            ctl.show_actions(id);
        });
    }
    {
        let c = c.clone();
        g.on_open_folder(move |id| {
            let ctl = c.borrow();
            let Some(inst) = parse_id(&id).and_then(|id| ctl.state.config.find(id)) else { return };
            ctl.close_sheet();
            if let Err(e) = shell::open_in_explorer(&inst.working_directory) {
                ctl.banner(&format!("Could not open {}: {e}", inst.working_directory.display()), true);
            }
        });
    }
    {
        let c = c.clone();
        g.on_create_shortcut(move |id, desktop| {
            let place = if desktop { ShortcutPlace::Desktop } else { ShortcutPlace::StartMenu };
            c.borrow_mut().shortcut(parse_id(&id), place, false);
        });
    }
    {
        let c = c.clone();
        g.on_launcher_shortcut(move |desktop| {
            let place = if desktop { ShortcutPlace::Desktop } else { ShortcutPlace::StartMenu };
            c.borrow_mut().shortcut(None, place, false);
        });
    }
    {
        let c = c.clone();
        g.on_remove(move |id| {
            if let Some(id) = parse_id(&id) {
                c.borrow_mut().ask_remove(id);
            }
        });
    }
    {
        let c = c.clone();
        g.on_confirm_accepted(move || {
            let mut ctl = c.borrow_mut();
            match ctl.confirm.take() {
                Some(Pending::Remove(id)) => ctl.remove(id, true),
                Some(Pending::RemoveSkippingSteam(id)) => ctl.remove(id, false),
                Some(Pending::ReplaceShortcut { id, place }) => ctl.shortcut(id, place, true),
                None => ctl.close_sheet(),
            }
        });
    }
}

impl Controller {
    fn show_actions(&self, id: Uuid) {
        let Some(ui) = self.ui() else { return };
        let list = &self.state.config.installations;
        let Some(pos) = list.iter().position(|i| i.id == id) else { return };
        let g = ui.global::<Ui>();
        g.set_actions_id(id.to_string().into());
        g.set_actions_title(list[pos].name.clone().into());
        g.set_actions_can_move_left(pos > 0);
        g.set_actions_can_move_right(pos + 1 < list.len());
        g.set_actions_has_steam(list[pos].steam_hook.is_some());
        g.set_sheet("actions".into());
    }

    fn confirm(&mut self, pending: Pending, title: &str, text: &str, button: &str, destructive: bool) {
        self.confirm = Some(pending);
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<Ui>();
        g.set_confirm_title(title.into());
        g.set_confirm_text(text.into());
        g.set_confirm_button(button.into());
        g.set_confirm_destructive(destructive);
        g.set_sheet("confirm".into());
    }

    fn ask_remove(&mut self, id: Uuid) {
        let Some(inst) = self.state.config.find(id) else { return };
        let mut text = format!(
            "{} will be removed from this launcher. The installation at {} is not touched.",
            inst.name,
            inst.working_directory.display()
        );
        let button = if inst.steam_hook.is_some() {
            text.push_str(" It is connected to Steam's Play button, so the game's previous Steam launch options are put back first. Steam must be closed for that.");
            "Restore and Remove"
        } else {
            "Remove"
        };
        let title = format!("Remove {}?", inst.name);
        self.confirm(Pending::Remove(id), &title, &text, button, true);
    }

    /// Removes an installation from the launcher. With `restore_steam`, its
    /// Steam Play hook is undone first, so Steam's Play button does not point
    /// at a modlist the launcher no longer knows.
    fn remove(&mut self, id: Uuid, restore_steam: bool) {
        let Some(name) = self.state.config.find(id).map(|i| i.name.clone()) else { return };
        let hook = self.state.config.find(id).and_then(|i| i.steam_hook.clone());
        let mut steam_note = "";
        if let (true, Some(hook)) = (restore_steam, hook) {
            match crate::steam_hooks::restore(
                &hook,
                self.state.steam.as_ref(),
                self.state.data(),
                crate::platform::steam::is_running(),
                |l| self.state.log(l),
            ) {
                Ok(crate::steam_hooks::Restored::Restored) => steam_note = " Steam's previous launch options are back.",
                Ok(crate::steam_hooks::Restored::LeftAsIs) => {
                    steam_note = " Its Steam launch options had been changed outside the launcher, so they were left as they are."
                }
                Err(crate::steam_hooks::RestoreError::SteamRunning) => {
                    self.close_sheet();
                    self.banner(
                        &format!(
                            "Exit Steam completely (Steam menu > Exit), then remove {name} again. Nothing was changed."
                        ),
                        true,
                    );
                    return;
                }
                Err(e) => {
                    let text = format!(
                        "Steam's launch options could not be restored: {e}\n\nRemove {name} anyway? Steam's Play button would then need its launch options cleared by hand: right-click the game > Properties > General > Launch Options."
                    );
                    self.confirm(Pending::RemoveSkippingSteam(id), "Remove anyway?", &text, "Remove Anyway", true);
                    return;
                }
            }
        }
        let Some(inst) = self.state.config.remove(id) else { return };
        if let Artwork::Custom { file } = &inst.artwork
            && let Ok(path) = self.state.data().resolve(file)
        {
            let _ = std::fs::remove_file(path);
        }
        self.art.forget(id);
        self.profile_models.remove(&id);
        self.state.inspections.remove(&id);
        self.close_sheet();
        if self.save_or_report() {
            self.banner(&format!("Removed {} from the launcher.{steam_note}", inst.name), false);
        }
        self.refresh_cards();
        // A Steam game skipped on removal still points here; Settings lists it.
        self.refresh_orphans();
    }

    /// Creates a shortcut to the launcher (`id` = None) or to one modlist.
    fn shortcut(&mut self, id: Option<Uuid>, place: ShortcutPlace, replace: bool) {
        let exe = current_exe();
        let workdir = exe.parent().map(PathBuf::from).unwrap_or_default();
        let (name, args, icon, description) = match id.and_then(|id| self.state.config.find(id)) {
            Some(inst) => {
                let install = self.state.inspections.get(&inst.id).and_then(|i| i.install.as_ref());
                // The game's own exe carries its real icon; a Play target such as
                // skse64_loader.exe usually has a poor one or none.
                let play_title = match &inst.play_action {
                    PlayAction::Executable { title } => Some(title.as_str()),
                    PlayAction::OpenMo2 => None,
                };
                let icon = install.map(|i| i.icon_source(play_title)).unwrap_or_else(|| inst.exe_path.clone());
                (
                    inst.name.clone(),
                    format!("--launch {}", inst.id),
                    icon,
                    format!("Start {} with Mod Organizer 2", inst.name),
                )
            }
            None if id.is_some() => return,
            None => {
                ("Modlist Launcher".to_string(), String::new(), exe.clone(), "Choose a modlist to start".to_string())
            }
        };
        let Some(path) = shell::shortcut_path(place, &name) else {
            self.banner("The shortcut folder could not be found.", true);
            return;
        };
        let shortcut = Shortcut {
            target: &exe,
            arguments: &args,
            working_directory: &workdir,
            icon: Some((&icon, 0)),
            description: &description,
        };
        let where_ = if place == ShortcutPlace::Desktop { "desktop" } else { "Start menu" };
        match shell::create_shortcut(&path, &shortcut, replace) {
            Ok(()) => {
                self.close_sheet();
                self.state.log(&format!("shortcut: {}", path.display()));
                self.banner(&format!("Added \"{name}\" to the {where_}."), false);
                self.shortcuts.refresh(true);
                self.refresh_cards();
            }
            Err(ShortcutError::Exists(p)) => {
                let text = format!(
                    "{} already exists. Replace it with a shortcut that starts {name} through Modlist Launcher?",
                    p.display()
                );
                self.confirm(Pending::ReplaceShortcut { id, place }, "Replace shortcut?", &text, "Replace", false);
            }
            Err(e) => {
                self.close_sheet();
                self.banner(&format!("Could not create the shortcut: {e}"), true);
            }
        }
    }
}
