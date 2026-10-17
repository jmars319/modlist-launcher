//! Scan sheet: discovery runs on a worker thread; nothing is added until the
//! user ticks candidates and confirms.

use std::path::PathBuf;
use std::rc::Rc;

use modlist_core::discovery::{self, Candidate, ScanOptions};
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
    fn open_scan(&mut self) {
        if self.scan.candidates.is_empty() && self.scan.timer.as_ref().is_some_and(slint::Timer::running) {
            self.open_sheet("scan");
            return;
        }
        self.scan = ScanDraft::default();
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
