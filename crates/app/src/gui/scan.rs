//! Scan sheet: discovery runs on a worker thread; nothing is added until the
//! user ticks candidates and confirms. At startup the saved scan folders are
//! scanned quietly, and new modlists are announced in the banner.

use std::path::PathBuf;
use std::rc::Rc;

use modlist_core::discovery::{self, Candidate, ScanOptions, ScanReport};
use modlist_core::paths::same_path;
use slint::{ComponentHandle, Model, VecModel};

use super::{Controller, Shared};
use crate::platform::dialogs::{self, Pick};
use crate::{AppWindow, ScanRow, ScanState, Ui};

#[derive(Default)]
pub struct ScanDraft {
    candidates: Vec<Candidate>,
    checked: Vec<bool>,
    timer: Option<slint::Timer>,
    /// The startup scan's timer, its result until reviewed or dismissed, and
    /// whether the banner is showing it (it waits behind another message).
    startup_timer: Option<slint::Timer>,
    startup: Option<ScanReport>,
    startup_shown: bool,
}

pub fn wire(ui: &AppWindow, c: &Shared) {
    let g = ui.global::<ScanState>();
    {
        let c = c.clone();
        ui.global::<Ui>().on_open_scan(move || c.borrow_mut().open_scan());
    }
    {
        let c = c.clone();
        g.on_add_root(move || {
            let owner = c.borrow().hwnd();
            if let Some(dir) = dialogs::pick(owner, "Choose a folder that holds modlists", Pick::Folder, None) {
                let mut ctl = c.borrow_mut();
                if !ctl.state.config.scan_roots.iter().any(|r| same_path(r, &dir)) {
                    ctl.state.config.scan_roots.push(dir);
                    ctl.save_or_report();
                }
                ctl.show_roots();
            }
        });
    }
    {
        let c = c.clone();
        g.on_remove_root(move |i| {
            let mut ctl = c.borrow_mut();
            if (i as usize) < ctl.state.config.scan_roots.len() {
                ctl.state.config.scan_roots.remove(i as usize);
                ctl.save_or_report();
            }
            ctl.show_roots();
        });
    }
    {
        let c = c.clone();
        g.on_start(move || start(&c));
    }
    {
        let c = c.clone();
        g.on_toggle(move |i, on| {
            let mut ctl = c.borrow_mut();
            if let Some(slot) = ctl.scan.checked.get_mut(i as usize) {
                *slot = on;
            }
            ctl.show_selection();
        });
    }
    {
        let c = c.clone();
        g.on_add_selected(move || c.borrow_mut().add_selected());
    }
    {
        let c = c.clone();
        ui.global::<Ui>().on_banner_action_clicked(move || c.borrow_mut().review_new_modlists());
    }
    {
        let c = c.clone();
        ui.global::<Ui>().on_banner_dismissed(move |was_notice| {
            let mut ctl = c.borrow_mut();
            if was_notice {
                ctl.dismiss_new_modlists();
            } else {
                ctl.show_pending_notice();
            }
        });
    }
}

/// Scans the saved scan folders in the background when the window opens.
/// Only announces; nothing is added without the user's OK.
pub fn startup_scan(c: &Shared) {
    let (roots, known) = {
        let ctl = c.borrow();
        let known: Vec<PathBuf> = ctl.state.config.installations.iter().map(|i| i.exe_path.clone()).collect();
        (ctl.state.config.scan_roots.clone(), known)
    };
    if roots.is_empty() {
        return;
    }
    let shared = Rc::downgrade(c);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let report = discovery::scan(&roots, ScanOptions::default(), |p| known.iter().any(|k| same_path(k, p)));
        let _ = tx.send(report);
    });
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(250), move || {
        let Some(c) = shared.upgrade() else { return };
        let Ok(mut ctl) = c.try_borrow_mut() else { return };
        let Ok(report) = rx.try_recv() else { return };
        if let Some(t) = &ctl.scan.startup_timer {
            t.stop();
        }
        ctl.announce_new_modlists(report);
    });
    c.borrow_mut().scan.startup_timer = Some(timer);
}

/// The banner text for modlists found at startup.
fn new_modlists_text(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("Found a new modlist in your scan folders: {one}."),
        [a, b] => format!("Found 2 new modlists in your scan folders: {a} and {b}."),
        more => format!("Found {} new modlists in your scan folders.", more.len()),
    }
}

fn start(c: &Shared) {
    let (roots, known) = {
        let ctl = c.borrow();
        let known: Vec<PathBuf> = ctl.state.config.installations.iter().map(|i| i.exe_path.clone()).collect();
        (ctl.state.config.scan_roots.clone(), known)
    };
    {
        let ctl = c.borrow();
        if let Some(ui) = ctl.ui() {
            let g = ui.global::<ScanState>();
            g.set_scanning(true);
            g.set_status("Scanning…".into());
            g.set_rows(Rc::new(VecModel::<ScanRow>::default()).into());
            g.set_selected_count(0);
        }
    }
    let shared = Rc::downgrade(c);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let report = discovery::scan(&roots, ScanOptions::default(), |p| known.iter().any(|k| same_path(k, p)));
        let _ = tx.send(report);
    });
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(150), move || {
        let Some(c) = shared.upgrade() else { return };
        let Ok(mut ctl) = c.try_borrow_mut() else { return };
        let Ok(report) = rx.try_recv() else { return };
        if let Some(t) = &ctl.scan.timer {
            t.stop();
        }
        let found = report.candidates.len();
        let mut status = match found {
            0 => "No Mod Organizer installations were found.".to_string(),
            1 => "Found 1 installation.".to_string(),
            n => format!("Found {n} installations."),
        };
        if report.truncated {
            status.push_str(
                " The scan stopped early because the folders are very large; choose a narrower folder to look deeper.",
            );
        }
        if !report.skipped.is_empty() {
            status.push_str(&format!(" {} folder(s) could not be read.", report.skipped.len()));
        }
        ctl.scan.checked = report.candidates.iter().map(Candidate::recommended).collect();
        ctl.scan.candidates = report.candidates;
        if let Some(ui) = ctl.ui() {
            let g = ui.global::<ScanState>();
            g.set_scanning(false);
            g.set_status(status.into());
        }
        ctl.show_rows();
    });
    c.borrow_mut().scan.timer = Some(timer);
}

impl Controller {
    fn announce_new_modlists(&mut self, report: ScanReport) {
        self.scan.startup = Some(report);
        self.scan.startup_shown = false;
        self.show_pending_notice();
    }

    /// Shows the startup notice if one is waiting and the banner is free. A
    /// problem or other message already showing is never covered; the notice
    /// follows once that one is dismissed.
    pub fn show_pending_notice(&mut self) {
        let Some(report) = &self.scan.startup else { return };
        if self.scan.startup_shown {
            return;
        }
        // Candidates already in the list may have been added since the scan began.
        let names: Vec<&str> = report
            .new_modlists(&self.state.config.dismissed_installs)
            .into_iter()
            .filter(|c| !self.state.config.contains_exe(&c.exe))
            .map(|c| c.suggested_name.as_str())
            .collect();
        if names.is_empty() {
            self.scan.startup = None;
            return;
        }
        let text = new_modlists_text(&names);
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<Ui>();
        if !g.get_banner_text().is_empty() {
            return;
        }
        g.set_banner_error(false);
        g.set_banner_text(text.into());
        g.set_banner_action("Review".into());
        self.scan.startup_shown = true;
    }

    /// Opens the scan sheet on the startup results, with the new ones ticked.
    fn review_new_modlists(&mut self) {
        let Some(report) = self.scan.startup.take() else { return };
        let new: Vec<PathBuf> =
            report.new_modlists(&self.state.config.dismissed_installs).iter().map(|c| c.exe.clone()).collect();
        let checked =
            report.candidates.iter().map(|c| new.contains(&c.exe) && !self.state.config.contains_exe(&c.exe)).collect();
        self.scan = ScanDraft { candidates: report.candidates, checked, ..Default::default() };
        if let Some(ui) = self.ui() {
            let g = ui.global::<Ui>();
            g.set_banner_text("".into());
            g.set_banner_action("".into());
            let s = ui.global::<ScanState>();
            s.set_scanning(false);
            s.set_status("Found at startup in your scan folders. Tick the ones to add.".into());
        }
        self.show_roots();
        self.show_rows();
        self.open_sheet("scan");
    }

    /// Remembers the announced modlists so the notice does not come back.
    fn dismiss_new_modlists(&mut self) {
        let Some(report) = self.scan.startup.take() else { return };
        let new: Vec<PathBuf> =
            report.new_modlists(&self.state.config.dismissed_installs).iter().map(|c| c.exe.clone()).collect();
        self.state.config.dismissed_installs.extend(new);
        self.save_or_report();
    }

    fn open_scan(&mut self) {
        if self.scan.candidates.is_empty() && self.scan.timer.as_ref().is_some_and(slint::Timer::running) {
            self.open_sheet("scan");
            return;
        }
        // The startup scan's result outlives a manual scan, so the banner's
        // Review button keeps working.
        self.scan = ScanDraft {
            startup_timer: self.scan.startup_timer.take(),
            startup: self.scan.startup.take(),
            startup_shown: self.scan.startup_shown,
            ..Default::default()
        };
        if let Some(ui) = self.ui() {
            let g = ui.global::<ScanState>();
            g.set_rows(Rc::new(VecModel::<ScanRow>::default()).into());
            g.set_status("".into());
            g.set_scanning(false);
            g.set_selected_count(0);
        }
        self.show_roots();
        self.open_sheet("scan");
    }

    fn show_roots(&self) {
        if let Some(ui) = self.ui() {
            let roots: Vec<slint::SharedString> =
                self.state.config.scan_roots.iter().map(|r| r.display().to_string().into()).collect();
            ui.global::<ScanState>().set_roots(Rc::new(VecModel::from(roots)).into());
        }
    }

    fn show_rows(&self) {
        let rows: Vec<ScanRow> = self
            .scan
            .candidates
            .iter()
            .zip(&self.scan.checked)
            .map(|(cand, &checked)| {
                let (note, error) = match &cand.install {
                    _ if cand.already_added => ("Already in the launcher.".to_string(), false),
                    Err(e) => (e.clone(), true),
                    Ok(i) if !i.portable => ("Not a portable install (no ModOrganizer.ini next to the exe). MO2 may ask which instance to open.".into(), false),
                    Ok(i) => (
                        [i.game_name.clone(), i.version.as_ref().map(|v| format!("MO2 {v}"))].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                        false,
                    ),
                };
                ScanRow {
                    checked,
                    selectable: !cand.already_added && cand.install.is_ok(),
                    name: cand.suggested_name.clone().into(),
                    path: cand.exe.parent().unwrap_or(&cand.exe).display().to_string().into(),
                    note: note.into(),
                    note_is_error: error,
                }
            })
            .collect();
        if let Some(ui) = self.ui() {
            ui.global::<ScanState>().set_rows(Rc::new(VecModel::from(rows)).into());
        }
        self.show_selection();
    }

    fn show_selection(&self) {
        if let Some(ui) = self.ui() {
            let g = ui.global::<ScanState>();
            let count = self.scan.checked.iter().filter(|&&c| c).count();
            g.set_selected_count(count as i32);
            let rows = g.get_rows();
            for (i, &checked) in self.scan.checked.iter().enumerate() {
                if let Some(mut row) = rows.row_data(i)
                    && row.checked != checked
                {
                    row.checked = checked;
                    rows.set_row_data(i, row);
                }
            }
        }
    }

    fn add_selected(&mut self) {
        let mut added = Vec::new();
        for (cand, &checked) in self.scan.candidates.iter().zip(&self.scan.checked) {
            if !checked || cand.already_added || self.state.config.contains_exe(&cand.exe) {
                continue;
            }
            let inst =
                super::edit::new_installation(cand.exe.clone(), cand.install.as_ref().ok(), self.state.steam.as_ref());
            added.push(inst.name.clone());
            self.state.config.installations.push(inst);
        }
        if added.is_empty() {
            return;
        }
        if self.save_or_report() {
            self.close_sheet();
            let msg = match added.len() {
                1 => format!("Added {}.", added[0]),
                n => format!("Added {n} installations."),
            };
            self.banner(&msg, false);
        }
        self.state.inspect_all();
        self.refresh_cards();
    }
}

#[cfg(test)]
mod tests {
    use super::new_modlists_text;

    #[test]
    fn names_new_modlists_in_the_banner() {
        assert_eq!(new_modlists_text(&[]), "");
        assert_eq!(new_modlists_text(&["Frostbound"]), "Found a new modlist in your scan folders: Frostbound.");
        assert_eq!(
            new_modlists_text(&["Frostbound", "Life in the Ruins"]),
            "Found 2 new modlists in your scan folders: Frostbound and Life in the Ruins."
        );
        assert_eq!(new_modlists_text(&["a", "b", "c"]), "Found 3 new modlists in your scan folders.");
    }
}
