//! Turns an installation and its current state into what a card shows.

use modlist_core::config::{Installation, PlayAction};
use modlist_core::process::{self, Gate, RunningProcess};
use modlist_core::validate::{Inspection, Severity, display_message};
use slint::{ModelRc, SharedString};

use crate::art::Art;
use crate::{CardStatus, InstallCard};

pub struct CardInput<'a> {
    pub inst: &'a Installation,
    pub inspection: Option<&'a Inspection>,
    pub art: Art,
    pub running: &'a [RunningProcess],
    pub all: &'a [Installation],
    pub starting: bool,
    pub profiles: ModelRc<SharedString>,
}

pub fn profile_options(inspection: Option<&Inspection>) -> Vec<String> {
    let install = inspection.and_then(|i| i.install.as_ref());
    let current = install.and_then(|i| i.selected_profile.clone());
    let mut out = vec![match current {
        Some(p) => format!("{p} (current)"),
        None => "MO2's current profile".to_string(),
    }];
    out.extend(install.map(|i| i.profiles.clone()).unwrap_or_default());
    out
}

pub fn profile_index(inst: &Installation, inspection: Option<&Inspection>) -> i32 {
    let profiles = inspection.and_then(|i| i.install.as_ref()).map(|i| i.profiles.as_slice()).unwrap_or_default();
    inst.profile.as_ref().and_then(|p| profiles.iter().position(|x| x == p)).map_or(0, |i| i as i32 + 1)
}

pub fn build(input: CardInput<'_>) -> InstallCard {
    let CardInput { inst, inspection, art, running, all, starting, profiles } = input;
    let install = inspection.and_then(|i| i.install.as_ref());
    let has_play_exe = matches!(inst.play_action, PlayAction::Executable { .. });
    let can_open_files = inspection.is_some_and(Inspection::can_open);
    let can_play_files = inspection.is_some_and(Inspection::can_play);
    let gate = process::gate(&inst.exe_path, running, all);

    let mut card = InstallCard {
        id: inst.id.to_string().into(),
        name: inst.name.clone().into(),
        game: install.and_then(|i| i.game_name.clone()).unwrap_or_else(|| "Mod Organizer 2".into()).into(),
        play_label: if has_play_exe { "Play" } else { "Open MO2" }.into(),
        show_open: has_play_exe,
        open_label: "Open MO2".into(),
        profiles,
        profile_index: profile_index(inst, inspection),
        has_hero: art.hero.is_some(),
        hero: art.hero.unwrap_or_default(),
        has_logo: art.logo.is_some(),
        logo: art.logo.unwrap_or_default(),
        has_icon: art.icon.is_some(),
        icon: art.icon.unwrap_or_default(),
        steam_linked: inst.steam_hook.is_some(),
        ..Default::default()
    };

    let first_error = inspection.and_then(|i| i.first_error()).map(|e| display_message(e).to_string());
    let first_warning =
        inspection.and_then(|i| i.issues.iter().find(|x| x.severity == Severity::Warning)).map(|w| w.message.clone());

    if starting {
        card.status = CardStatus::Starting;
        card.status_text = "Starting".into();
        card.detail = "Starting Mod Organizer…".into();
        return card;
    }
    if !can_open_files {
        card.status = CardStatus::Attention;
        card.status_text = "Needs attention".into();
        card.detail = first_error.unwrap_or_else(|| "This installation could not be checked.".into()).into();
        card.detail_is_error = true;
        return card;
    }
    match gate {
        Gate::SameInstanceRunning { .. } => {
            card.status = CardStatus::Running;
            card.status_text = "Running".into();
            card.detail = "Mod Organizer is open for this modlist.".into();
            card.can_play = can_play_files;
            card.can_open = true;
            card.open_label = "Show MO2".into();
            if !has_play_exe {
                card.play_label = "Show MO2".into();
                card.can_play = true;
            }
        }
        Gate::OtherInstanceRunning { name, .. } => {
            card.status = CardStatus::Blocked;
            card.status_text = "Waiting".into();
            card.detail = match name {
                Some(n) => format!("Close Mod Organizer for {n} first. MO2 runs one instance at a time."),
                None => "Close the other Mod Organizer first. MO2 runs one instance at a time.".into(),
            }
            .into();
        }
        Gate::Clear => {
            card.can_open = true;
            card.can_play = can_play_files || !has_play_exe;
            if let Some(err) = first_error {
                card.status = CardStatus::Attention;
                card.status_text = "Needs attention".into();
                card.detail = err.into();
                card.detail_is_error = true;
            } else {
                card.status = CardStatus::Ready;
                card.status_text = "Ready".into();
                card.detail = first_warning.unwrap_or_default().into();
            }
        }
    }
    card
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use modlist_core::validate::Issue;

    use super::*;

    fn installs() -> Vec<Installation> {
        let mut a =
            Installation::new("Project Ultrapunk", PathBuf::from(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe"));
        a.play_action = PlayAction::Executable { title: "Cyberpunk 2077".into() };
        let b = Installation::new("Witcher 3 Ascended", PathBuf::from(r"C:\Modding\W3\ModOrganizer.exe"));
        vec![a, b]
    }

    fn mo2(path: &str) -> RunningProcess {
        RunningProcess { pid: 42, path: Some(PathBuf::from(path)), name: "ModOrganizer.exe".into() }
    }

    fn card(inst: &Installation, issues: Vec<Issue>, running: &[RunningProcess], all: &[Installation]) -> InstallCard {
        let inspection = Inspection { install: None, issues };
        build(CardInput {
            inst,
            inspection: Some(&inspection),
            art: Art::default(),
            running,
            all,
            starting: false,
            profiles: ModelRc::default(),
        })
    }

    fn issue(severity: Severity, message: &str) -> Issue {
        Issue { severity, message: message.into() }
    }

    #[test]
    fn ready_with_play_action_shows_both_buttons() {
        let all = installs();
        let c = card(&all[0], vec![], &[], &all);
        assert_eq!(c.status, CardStatus::Ready);
        assert_eq!(c.play_label, "Play");
        assert!(c.show_open && c.can_play && c.can_open);
    }

    #[test]
    fn open_only_installation_has_single_button() {
        let all = installs();
        let c = card(&all[1], vec![], &[], &all);
        assert_eq!(c.play_label, "Open MO2");
        assert!(!c.show_open && c.can_play);
    }

    #[test]
    fn same_instance_running_offers_show() {
        let all = installs();
        let c = card(&all[0], vec![], &[mo2(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")], &all);
        assert_eq!(c.status, CardStatus::Running);
        assert_eq!(c.open_label, "Show MO2");
        assert!(c.can_play && c.can_open);
    }

    #[test]
    fn other_instance_running_blocks_with_name() {
        let all = installs();
        let c = card(&all[1], vec![], &[mo2(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")], &all);
        assert_eq!(c.status, CardStatus::Blocked);
        assert!(!c.can_play && !c.can_open);
        assert!(c.detail.contains("Project Ultrapunk"));
    }

    #[test]
    fn missing_install_needs_attention() {
        let all = installs();
        let c = card(&all[0], vec![issue(Severity::Error, r"The folder D:\Gone no longer exists.")], &[], &all);
        assert_eq!(c.status, CardStatus::Attention);
        assert!(c.detail_is_error && !c.can_play && !c.can_open);
    }

    #[test]
    fn play_only_error_still_allows_opening_mo2() {
        let all = installs();
        let c = card(&all[0], vec![issue(Severity::Error, r"Play: C:\x.exe was not found.")], &[], &all);
        assert_eq!(c.status, CardStatus::Attention);
        assert!(!c.can_play && c.can_open);
        assert_eq!(c.detail, r"C:\x.exe was not found.");
    }

    #[test]
    fn warnings_show_but_do_not_block() {
        let all = installs();
        let c = card(&all[0], vec![issue(Severity::Warning, "Profile \"X\" no longer exists.")], &[], &all);
        assert_eq!(c.status, CardStatus::Ready);
        assert!(!c.detail_is_error && c.can_play);
    }
}
