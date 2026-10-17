//! Steam Play button sheet. Writes exactly one value in one account's
//! `localconfig.vdf`, only on Apply or Restore, only while Steam is closed,
//! and only after a backup copy is saved in the launcher's data folder.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use modlist_core::config::{SteamHook, timestamp};
use modlist_core::steam::{self, Account};
use slint::{ComponentHandle, SharedString, VecModel};
use uuid::Uuid;

use super::{Controller, Shared};
use crate::state::current_exe;
use crate::{AppWindow, SteamState, Ui};

#[derive(Default)]
pub struct SteamDraft {
    id: Option<Uuid>,
    accounts: Vec<Account>,
}

pub fn wire(ui: &AppWindow, c: &Shared) {
    let g = ui.global::<SteamState>();
    {
        let c = c.clone();
        ui.global::<Ui>().on_steam_setup(move |id| {
            if let Some(id) = crate::state::parse_id(&id) {
                c.borrow_mut().open_steam(id);
            }
        });
    }
    {
        let c = c.clone();
        g.on_account_changed(move |_| c.borrow_mut().steam_refresh(None));
    }
    {
        let c = c.clone();
        g.on_app_id_changed(move || c.borrow_mut().steam_refresh(None));
    }
    {
        let c = c.clone();
        g.on_recheck(move || c.borrow_mut().steam_refresh(None));
    }
    {
        let c = c.clone();
        g.on_apply(move || {
            let mut ctl = c.borrow_mut();
            let result = ctl.steam_apply();
            ctl.steam_refresh(Some(result));
        });
    }
    {
        let c = c.clone();
        g.on_restore(move || {
            let mut ctl = c.borrow_mut();
            let result = ctl.steam_restore();
            ctl.steam_refresh(Some(result));
        });
    }
    {
        let c = c.clone();
        g.on_copy(move || {
            let ctl = c.borrow();
            let Some(ui) = ctl.ui() else { return };
            let g = ui.global::<SteamState>();
            let ok = crate::platform::clipboard::set_text(g.get_proposed().as_str());
            g.set_status_error(!ok);
            g.set_status(
                if ok {
                    "Copied. In Steam, right-click the game > Properties > General > Launch Options, and paste."
                } else {
                    "Could not copy to the clipboard."
                }
                .into(),
            );
        });
    }
}

fn write_steam_file(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("vdf.modlist-launcher-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

impl Controller {
    fn open_steam(&mut self, id: Uuid) {
        let Some(inst) = self.state.config.find(id).cloned() else { return };
        self.steam =
            SteamDraft { id: Some(id), accounts: self.state.steam.as_ref().map(|s| s.accounts()).unwrap_or_default() };
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<SteamState>();
        g.set_title(format!("Steam Play Button: {}", inst.name).into());
        g.set_intro("Lets Steam's Play button start this modlist through Mod Organizer.".into());
        let labels: Vec<SharedString> = self.steam.accounts.iter().map(|a| a.label().into()).collect();
        g.set_accounts(Rc::new(VecModel::from(labels)).into());
        let account_index = inst
            .steam_hook
            .as_ref()
            .and_then(|h| self.steam.accounts.iter().position(|a| a.account_id == h.account_id))
            .unwrap_or(0);
        g.set_account_index(account_index as i32);
        let app = inst.steam_hook.as_ref().map(|h| h.app_id).or(inst.steam_app_id);
        g.set_app_id(app.map(|a| a.to_string()).unwrap_or_default().into());
        g.set_status("".into());
        self.steam_refresh(None);
        self.open_sheet("steam");
    }

    fn selected_account(&self) -> Option<&Account> {
        let i = self.ui()?.global::<SteamState>().get_account_index().max(0) as usize;
        self.steam.accounts.get(i)
    }

    fn selected_app(&self) -> Option<u32> {
        self.ui()?.global::<SteamState>().get_app_id().trim().parse().ok().filter(|&a: &u32| a > 0)
    }

    /// Re-reads Steam's current state. `result` is the outcome of an action
    /// just taken, shown in the status line.
    fn steam_refresh(&mut self, result: Option<Result<String, String>>) {
        let Some(id) = self.steam.id else { return };
        let Some(inst) = self.state.config.find(id).cloned() else { return };
        let running = crate::platform::steam::is_running();
        let proposed = steam::wrapper_launch_options(&current_exe(), &id.to_string());
        let mut problem = None;
        let current = match (self.state.steam.as_ref(), self.selected_account(), self.selected_app()) {
            (None, _, _) => {
                problem = Some("Steam was not found on this PC.".to_string());
                String::new()
            }
            (_, None, _) => {
                problem = Some(
                    "No Steam account with local settings was found. Sign in to Steam once, then try again.".into(),
                );
                String::new()
            }
            (_, _, None) => {
                problem = Some("Enter the game's Steam app ID.".into());
                String::new()
            }
            (Some(_), Some(acct), Some(app)) => match std::fs::read_to_string(&acct.localconfig) {
                Ok(text) => steam::get_launch_options(&text, app).unwrap_or_else(|e| {
                    problem = Some(format!("Steam's settings file could not be read: {e}"));
                    String::new()
                }),
                Err(e) => {
                    problem = Some(format!("Could not read {}: {e}", acct.localconfig.display()));
                    String::new()
                }
            },
        };
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<SteamState>();
        g.set_steam_running(running && problem.is_none());
        g.set_current(current.clone().into());
        g.set_proposed(proposed.clone().into());
        g.set_hooked(inst.steam_hook.is_some());
        g.set_can_apply(problem.is_none() && current != proposed);
        match (result, problem) {
            (Some(Ok(msg)), _) => {
                g.set_status_error(false);
                g.set_status(msg.into());
            }
            (Some(Err(msg)), _) | (None, Some(msg)) => {
                g.set_status_error(true);
                g.set_status(msg.into());
            }
            (None, None) => {
                g.set_status_error(false);
                g.set_status(
                    if current == proposed { "Steam's Play button already starts this modlist." } else { "" }.into(),
                );
            }
        }
    }

    fn backup_steam_file(&self, acct: &Account, text: &str) -> Result<PathBuf, String> {
        let rel = PathBuf::from("steam-backups").join(format!("localconfig-{}-{}.vdf", acct.account_id, timestamp()));
        self.state
            .data()
            .write_atomic(&rel, text.as_bytes())
            .map_err(|e| format!("Could not back up Steam's settings, so nothing was changed: {e}"))
    }

    fn steam_apply(&mut self) -> Result<String, String> {
        let id = self.steam.id.ok_or("No modlist selected.")?;
        if crate::platform::steam::is_running() {
            return Err(
                "Steam is still running. Exit Steam completely (Steam menu > Exit), then press Apply again.".into()
            );
        }
        let acct = self.selected_account().cloned().ok_or("Choose a Steam account.")?;
        let app = self.selected_app().ok_or("Enter the game's Steam app ID.")?;
        let proposed = steam::wrapper_launch_options(&current_exe(), &id.to_string());
        let text =
            std::fs::read_to_string(&acct.localconfig).map_err(|e| format!("Could not read Steam's settings: {e}"))?;
        let current = steam::get_launch_options(&text, app).map_err(|e| e.to_string())?;
        if current != proposed {
            let backup = self.backup_steam_file(&acct, &text)?;
            let updated = steam::set_launch_options(&text, app, &proposed).map_err(|e| e.to_string())?;
            write_steam_file(&acct.localconfig, &updated)
                .map_err(|e| format!("Could not write Steam's settings: {e}"))?;
            let verify =
                std::fs::read_to_string(&acct.localconfig).ok().and_then(|t| steam::get_launch_options(&t, app).ok());
            if verify.as_deref() != Some(proposed.as_str()) {
                let _ = write_steam_file(&acct.localconfig, &text);
                return Err("Steam's settings did not read back correctly, so the original file was put back.".into());
            }
            self.state.log(&format!(
                "steam: set launch options for app {app} (account {}); backup {}",
                acct.account_id,
                backup.display()
            ));
        }
        let inst = self.state.config.find_mut(id).ok_or("The modlist was removed.")?;
        let previous = match &inst.steam_hook {
            Some(h) if h.account_id == acct.account_id && h.app_id == app => h.previous_launch_options.clone(),
            _ => current,
        };
        inst.steam_hook = Some(SteamHook {
            account_id: acct.account_id,
            app_id: app,
            previous_launch_options: previous,
            applied_launch_options: proposed,
        });
        if inst.steam_app_id.is_none() {
            inst.steam_app_id = Some(app);
        }
        self.save_or_report();
        self.refresh_cards();
        Ok("Done. Start Steam and press Play. Hold Shift while pressing Play to choose with or without mods.".into())
    }

    fn steam_restore(&mut self) -> Result<String, String> {
        let id = self.steam.id.ok_or("No modlist selected.")?;
        if crate::platform::steam::is_running() {
            return Err("Steam is still running. Exit Steam completely, then press Restore again.".into());
        }
        let hook = self.state.config.find(id).and_then(|i| i.steam_hook.clone()).ok_or("Nothing to restore.")?;
        let acct = self
            .steam
            .accounts
            .iter()
            .find(|a| a.account_id == hook.account_id)
            .cloned()
            .ok_or("The Steam account used before was not found.")?;
        let text =
            std::fs::read_to_string(&acct.localconfig).map_err(|e| format!("Could not read Steam's settings: {e}"))?;
        let current = steam::get_launch_options(&text, hook.app_id).map_err(|e| e.to_string())?;
        let message = if current != hook.applied_launch_options {
            "Steam's launch options were changed outside the launcher, so they were left as they are.".to_string()
        } else {
            let backup = self.backup_steam_file(&acct, &text)?;
            let updated = steam::set_launch_options(&text, hook.app_id, &hook.previous_launch_options)
                .map_err(|e| e.to_string())?;
            write_steam_file(&acct.localconfig, &updated)
                .map_err(|e| format!("Could not write Steam's settings: {e}"))?;
            self.state.log(&format!(
                "steam: restored launch options for app {}; backup {}",
                hook.app_id,
                backup.display()
            ));
            "Restored Steam's previous launch options.".to_string()
        };
        if let Some(inst) = self.state.config.find_mut(id) {
            inst.steam_hook = None;
        }
        self.save_or_report();
        self.refresh_cards();
        Ok(message)
    }
}
