//! Add / Edit sheet.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use modlist_core::config::{Artwork, Installation, PlayAction};
use modlist_core::discovery::suggest_name;
use modlist_core::mo2::{self, Mo2Install};
use modlist_core::paths::same_path;
use modlist_core::validate::{self, Severity};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use uuid::Uuid;

use super::{Controller, Shared};
use crate::platform::dialogs::{self, Pick};
use crate::{AppWindow, EditState, Message};

#[derive(Default)]
pub struct EditDraft {
    id: Option<Uuid>,
    install: Option<Mo2Install>,
    read_from: Option<PathBuf>,
    /// Index 0 is "Open Mod Organizer".
    play_titles: Vec<String>,
    /// Index 0 is "MO2's current profile".
    profiles: Vec<String>,
    existing_artwork: Artwork,
    new_artwork: Option<PathBuf>,
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(items.iter().map(SharedString::from).collect::<Vec<_>>())).into()
}

pub fn wire(ui: &AppWindow, c: &Shared) {
    let g = ui.global::<EditState>();
    let u = ui.global::<crate::Ui>();
    {
        let c = c.clone();
        u.on_add_installation(move || add(&c));
    }
    {
        let c = c.clone();
        u.on_edit(move |id| {
            if let Some(id) = crate::state::parse_id(&id) {
                c.borrow_mut().open_edit(id);
            }
        });
    }
    {
        let c = c.clone();
        g.on_changed(move || c.borrow_mut().edit_changed());
    }
    {
        let c = c.clone();
        g.on_browse_exe(move || {
            let (owner, start) = {
                let ctl = c.borrow();
                let start = ctl.ui().map(|u| PathBuf::from(u.global::<EditState>().get_exe_path().as_str()));
                (ctl.hwnd(), start.and_then(|p| p.parent().map(Path::to_path_buf)))
            };
            if let Some(path) = dialogs::pick(
                owner,
                "Choose ModOrganizer.exe",
                Pick::File(&[("Mod Organizer", "ModOrganizer.exe"), ("Programs", "*.exe")]),
                start.as_deref(),
            ) {
                let ctl = c.borrow();
                if let Some(ui) = ctl.ui() {
                    let g = ui.global::<EditState>();
                    g.set_exe_path(path.display().to_string().into());
                    if let Some(dir) = path.parent() {
                        g.set_working_dir(dir.display().to_string().into());
                    }
                }
                drop(ctl);
                c.borrow_mut().edit_changed();
            }
        });
    }
    {
        let c = c.clone();
        g.on_browse_working_dir(move || {
            let (owner, start) = {
                let ctl = c.borrow();
                (ctl.hwnd(), ctl.ui().map(|u| PathBuf::from(u.global::<EditState>().get_working_dir().as_str())))
            };
            if let Some(path) = dialogs::pick(owner, "Choose the working directory", Pick::Folder, start.as_deref()) {
                if let Some(ui) = c.borrow().ui() {
                    ui.global::<EditState>().set_working_dir(path.display().to_string().into());
                }
                c.borrow_mut().edit_changed();
            }
        });
    }
    {
        let c = c.clone();
        g.on_reset_working_dir(move || {
            if let Some(ui) = c.borrow().ui() {
                let g = ui.global::<EditState>();
                if let Some(dir) = Path::new(g.get_exe_path().as_str()).parent() {
                    g.set_working_dir(dir.display().to_string().into());
                }
            }
            c.borrow_mut().edit_changed();
        });
    }
    {
        let c = c.clone();
        g.on_browse_artwork(move || {
            let owner = c.borrow().hwnd();
            if let Some(path) =
                dialogs::pick(owner, "Choose artwork", Pick::File(&[("Images", "*.png;*.jpg;*.jpeg")]), None)
            {
                let mut ctl = c.borrow_mut();
                if slint::Image::load_from_path(&path).is_err() {
                    ctl.banner(&format!("{} could not be read as an image.", path.display()), true);
                    return;
                }
                ctl.edit.new_artwork = Some(path);
                if let Some(ui) = ctl.ui() {
                    ui.global::<EditState>().set_artwork_index(1);
                }
                ctl.edit_changed();
            }
        });
    }
    {
        let c = c.clone();
        g.on_save(move || c.borrow_mut().edit_save());
    }
}

fn add(c: &Shared) {
    let owner = c.borrow().hwnd();
    let Some(exe) = dialogs::pick(
        owner,
        "Choose the ModOrganizer.exe of a modlist",
        Pick::File(&[("Mod Organizer", "ModOrganizer.exe"), ("Programs", "*.exe")]),
        None,
    ) else {
        return;
    };
    c.borrow_mut().open_new(exe);
}

/// Defaults for a newly found installation: name, Play executable, Steam app.
pub fn new_installation(
    exe: PathBuf,
    install: Option<&Mo2Install>,
    steam: Option<&modlist_core::steam::SteamRoot>,
) -> Installation {
    let mut inst = Installation::new(suggest_name(&exe, install), exe);
    if let Some(install) = install {
        let play_title = install.default_play_executable().map(|e| e.title.clone());
        if let Some(title) = &play_title {
            inst.play_action = PlayAction::Executable { title: title.clone() };
        }
        let installed = steam.map(|s| s.installed_apps()).unwrap_or_default();
        inst.steam_app_id = modlist_core::steam::guess_app_id(install, play_title.as_deref(), &installed);
    }
    inst
}

impl Controller {
    fn open_new(&mut self, exe: PathBuf) {
        if let Some(existing) = self.state.config.installations.iter().find(|i| same_path(&i.exe_path, &exe)) {
            let msg = format!("{} is already in the launcher as \"{}\".", exe.display(), existing.name);
            self.banner(&msg, false);
            return;
        }
        let install = mo2::read_install(&exe).ok();
        let inst = new_installation(exe, install.as_ref(), self.state.steam.as_ref());
        self.fill_edit(&inst, true);
    }

    fn open_edit(&mut self, id: Uuid) {
        let Some(inst) = self.state.config.find(id).cloned() else { return };
        self.fill_edit(&inst, false);
    }

    fn fill_edit(&mut self, inst: &Installation, is_new: bool) {
        self.edit = EditDraft {
            id: (!is_new).then_some(inst.id),
            existing_artwork: inst.artwork.clone(),
            ..Default::default()
        };
        let Some(ui) = self.ui() else { return };
        let g = ui.global::<EditState>();
        g.set_is_new(is_new);
        g.set_name(inst.name.clone().into());
        g.set_exe_path(inst.exe_path.display().to_string().into());
        g.set_working_dir(inst.working_directory.display().to_string().into());
        g.set_steam_app_id(inst.steam_app_id.map(|a| a.to_string()).unwrap_or_default().into());
        g.set_artwork_options(strings(&["Automatic".into(), "Custom image".into(), "None".into()]));
        g.set_artwork_index(match inst.artwork {
            Artwork::Automatic => 0,
            Artwork::Custom { .. } => 1,
            Artwork::None => 2,
        });
        self.reload_install(Path::new(&inst.exe_path));
        let play = match &inst.play_action {
            PlayAction::Executable { title } => self.edit.play_titles.iter().position(|t| t == title).unwrap_or(0),
            PlayAction::OpenMo2 => 0,
        };
        let profile = inst.profile.as_ref().and_then(|p| self.edit.profiles.iter().position(|x| x == p)).unwrap_or(0);
        g.set_play_index(play as i32);
        g.set_profile_index(profile as i32);
        self.edit_changed();
        self.open_sheet("edit");
    }

    fn reload_install(&mut self, exe: &Path) {
        let install = mo2::read_install(exe).ok();
        let mut play_titles = vec!["Open Mod Organizer".to_string()];
        let mut profiles = vec!["MO2's current profile".to_string()];
        if let Some(i) = &install {
            play_titles.extend(i.executables.iter().filter(|e| !e.hidden).map(|e| e.title.clone()));
            profiles.extend(i.profiles.iter().cloned());
        }
        if let Some(ui) = self.ui() {
            let g = ui.global::<EditState>();
            let keep_play = self.edit.play_titles.get(g.get_play_index() as usize).cloned();
            let keep_profile = self.edit.profiles.get(g.get_profile_index() as usize).cloned();
            g.set_play_options(strings(&play_titles));
            g.set_profile_options(strings(&profiles));
            g.set_play_index(keep_play.and_then(|t| play_titles.iter().position(|x| *x == t)).unwrap_or(0) as i32);
            g.set_profile_index(keep_profile.and_then(|t| profiles.iter().position(|x| *x == t)).unwrap_or(0) as i32);
        }
        self.edit.play_titles = play_titles;
        self.edit.profiles = profiles;
        self.edit.install = install;
        self.edit.read_from = Some(exe.to_path_buf());
    }

    /// Builds the installation the form currently describes.
    fn draft_installation(&self) -> Option<Installation> {
        let ui = self.ui()?;
        let g = ui.global::<EditState>();
        let exe = PathBuf::from(g.get_exe_path().trim());
        let mut inst = match self.edit.id.and_then(|id| self.state.config.find(id)) {
            Some(existing) => existing.clone(),
            None => Installation::new("", exe.clone()),
        };
        inst.name = g.get_name().trim().to_string();
        inst.exe_path = exe;
        inst.working_directory = PathBuf::from(g.get_working_dir().trim());
        let play = g.get_play_index().max(0) as usize;
        inst.play_action = match self.edit.play_titles.get(play) {
            Some(title) if play > 0 => PlayAction::Executable { title: title.clone() },
            _ => PlayAction::OpenMo2,
        };
        let profile = g.get_profile_index().max(0) as usize;
        inst.profile = (profile > 0).then(|| self.edit.profiles.get(profile).cloned()).flatten();
        inst.steam_app_id = g.get_steam_app_id().trim().parse().ok().filter(|&a: &u32| a > 0);
        inst.artwork = match g.get_artwork_index() {
            2 => Artwork::None,
            1 if self.edit.new_artwork.is_some() => Artwork::Custom { file: PathBuf::from("pending") },
            1 => match &self.edit.existing_artwork {
                a @ Artwork::Custom { .. } => a.clone(),
                _ => Artwork::Automatic,
            },
            _ => Artwork::Automatic,
        };
        Some(inst)
    }

    pub(super) fn edit_changed(&mut self) {
        let exe_text = self.ui().map(|u| u.global::<EditState>().get_exe_path().trim().to_string()).unwrap_or_default();
        let exe = PathBuf::from(&exe_text);
        if self.edit.read_from.as_deref().is_none_or(|p| !same_path(p, &exe)) {
            self.reload_install(&exe);
        }
        let Some(inst) = self.draft_installation() else { return };
        let mut messages = Vec::new();
        let mut blocking = false;
        if inst.name.is_empty() {
            messages.push(Message { text: "Enter a name.".into(), error: true });
            blocking = true;
        }
        if let Some(dupe) = self
            .state
            .config
            .installations
            .iter()
            .find(|i| Some(i.id) != self.edit.id && same_path(&i.exe_path, &inst.exe_path))
        {
            messages.push(Message {
                text: format!("This installation is already in the launcher as \"{}\".", dupe.name).into(),
                error: true,
            });
            blocking = true;
        }
        if exe_text.is_empty() {
            messages.push(Message { text: "Choose the installation's ModOrganizer.exe.".into(), error: true });
            blocking = true;
        } else {
            let insp = validate::inspect(&inst);
            if !insp.can_open() {
                blocking = true;
            }
            for issue in &insp.issues {
                messages.push(Message {
                    text: validate::display_message(issue).to_string().into(),
                    error: issue.severity == Severity::Error,
                });
            }
        }
        if matches!(inst.artwork, Artwork::Custom { .. })
            && self.edit.new_artwork.is_none()
            && !matches!(self.edit.existing_artwork, Artwork::Custom { .. })
        {
            messages.push(Message { text: "Choose an image for custom artwork.".into(), error: true });
            blocking = true;
        }
        if let Some(ui) = self.ui() {
            let g = ui.global::<EditState>();
            g.set_messages(Rc::new(VecModel::from(messages)).into());
            g.set_can_save(!blocking);
        }
    }

    fn edit_save(&mut self) {
        let Some(mut inst) = self.draft_installation() else { return };
        if let Some(src) = self.edit.new_artwork.clone() {
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
            let rel = PathBuf::from("artwork").join(format!("{}.{ext}", inst.id));
            match self.state.data().copy_in(&src, &rel) {
                Ok(_) => inst.artwork = Artwork::Custom { file: rel },
                Err(e) => {
                    self.banner(&format!("Could not copy the artwork: {e}"), true);
                    return;
                }
            }
        }
        let id = inst.id;
        match self.state.config.find_mut(id) {
            Some(existing) => *existing = inst,
            None => self.state.config.installations.push(inst),
        }
        self.art.forget(id);
        if self.save_or_report() {
            self.close_sheet();
        }
        self.state.inspect(id);
        self.refresh_cards();
    }
}
