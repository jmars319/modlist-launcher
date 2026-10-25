//! Turns an installation and its current state into what a card shows.

use modlist_core::config::{Installation, PlayAction};
use modlist_core::process::{self, GameState, Gate, RunningProcess};
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
    pub game: GameState,
    /// How long this installation's MO2 has run with no window, once it
    /// counts as stuck.
    pub mo2_stuck: Option<std::time::Duration>,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
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

/// What the card's setup chips say, for screen readers.
fn setup_text(steam: bool, desktop: bool, start_menu: bool) -> String {
    let parts: Vec<&str> = [
        (steam, "Steam's Play button starts this modlist"),
        (desktop, "desktop shortcut"),
        (start_menu, "Start menu shortcut"),
    ]
    .into_iter()
    .filter_map(|(on, text)| on.then_some(text))
    .collect();
    if parts.is_empty() { String::new() } else { format!("Set up: {}.", parts.join(", ")) }
}

pub fn build(input: CardInput<'_>) -> InstallCard {
    let CardInput {
        inst,
        inspection,
        art,
        running,
        all,
        starting,
        game,
        mo2_stuck,
        desktop_shortcut,
        start_menu_shortcut,
        profiles,
    } = input;
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
        desktop_shortcut,
        start_menu_shortcut,
        setup_text: setup_text(inst.steam_hook.is_some(), desktop_shortcut, start_menu_shortcut).into(),
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
    if game != GameState::NotRunning {
        let name = card.game.to_string();
        let (status, text, detail) = match game {
            GameState::Running { .. } => (CardStatus::Running, "Playing", format!("{name} is running.")),
            GameState::Starting { .. } => (CardStatus::Starting, "Starting", format!("{name} is starting.")),
            GameState::Closing { .. } => (CardStatus::Blocked, "Closing", format!("{name} is still closing.")),
            _ => (CardStatus::Blocked, "Busy", format!("{name} is starting or closing.")),
        };
        card.status = status;
        card.status_text = text.into();
        card.detail = detail.into();
        card.can_play = false;
        match gate {
            Gate::SameInstanceRunning { .. } => {
                card.can_open = true;
                card.open_label = "Show MO2".into();
            }
            Gate::OtherInstanceRunning { .. } => card.can_open = false,
            Gate::Clear => card.can_open = true,
        }
        return card;
    }
    match gate {
        Gate::SameInstanceRunning { .. } if mo2_stuck.is_some() => {
            let how_long = process::describe_duration(mo2_stuck.unwrap_or_default());
            card.status = CardStatus::Attention;
            card.status_text = "Not responding".into();
            card.detail = format!(
                "Mod Organizer has been running for {how_long} with no window, so it is probably stuck closing. End MO2 stops it."
            )
            .into();
            card.detail_is_error = true;
            card.can_play = !has_play_exe;
            card.can_open = true;
            card.open_label = "End MO2".into();
            if !has_play_exe {
                card.play_label = "End MO2".into();
            }
        }
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
            game: GameState::NotRunning,
            mo2_stuck: None,
            desktop_shortcut: false,
            start_menu_shortcut: false,
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
    fn stuck_mo2_offers_end_instead_of_play() {
        let all = installs();
        let running = [mo2(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")];
        let inspection = Inspection { install: None, issues: vec![] };
        let input = |inst, stuck| CardInput {
            inst,
            inspection: Some(&inspection),
            art: Art::default(),
            running: &running,
            all: &all,
            starting: false,
            game: GameState::NotRunning,
            mo2_stuck: stuck,
            desktop_shortcut: false,
            start_menu_shortcut: false,
            profiles: ModelRc::default(),
        };
        // Ultrapunk's Play runs a game, so Play is disabled and Open becomes End.
        let c = build(input(&all[0], Some(std::time::Duration::from_secs(14 * 3600))));
        assert_eq!(c.status, CardStatus::Attention);
        assert_eq!(c.status_text, "Not responding");
        assert!(c.detail.contains("14 hours with no window"), "{}", c.detail);
        assert!(!c.can_play && c.can_open);
        assert_eq!(c.open_label, "End MO2");
        // A modlist whose Play only opens MO2 gets End MO2 on the Play button.
        let mut open_only = all[0].clone();
        open_only.play_action = PlayAction::OpenMo2;
        let c = build(input(&open_only, Some(std::time::Duration::from_secs(120))));
        assert_eq!(c.play_label, "End MO2");
        assert!(c.can_play);
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

    fn card_with_game(
        inst: &Installation,
        running: &[RunningProcess],
        all: &[Installation],
        game: GameState,
    ) -> InstallCard {
        let inspection = Inspection { install: None, issues: vec![] };
        build(CardInput {
            inst,
            inspection: Some(&inspection),
            art: Art::default(),
            running,
            all,
            starting: false,
            game,
            mo2_stuck: None,
            desktop_shortcut: false,
            start_menu_shortcut: false,
            profiles: ModelRc::default(),
        })
    }

    #[test]
    fn setup_chips_are_described_for_screen_readers() {
        assert_eq!(setup_text(false, false, false), "");
        assert_eq!(setup_text(false, true, false), "Set up: desktop shortcut.");
        assert_eq!(
            setup_text(true, true, true),
            "Set up: Steam's Play button starts this modlist, desktop shortcut, Start menu shortcut."
        );
    }

    #[test]
    fn game_states_block_play_with_a_clear_reason() {
        let all = installs();
        let cases = [
            (GameState::Running { pid: 1 }, CardStatus::Running, "Playing", "is running"),
            (GameState::Starting { pid: 1 }, CardStatus::Starting, "Starting", "is starting"),
            (GameState::Closing { pid: 1 }, CardStatus::Blocked, "Closing", "is still closing"),
            (GameState::Busy { pid: 1 }, CardStatus::Blocked, "Busy", "starting or closing"),
        ];
        for (game, status, text, detail) in cases {
            let c = card_with_game(&all[0], &[], &all, game);
            assert_eq!(c.status, status, "{game:?}");
            assert_eq!(c.status_text, text);
            assert!(c.detail.contains(detail), "{}", c.detail);
            assert!(!c.can_play, "{game:?} must block Play");
            assert!(c.can_open, "MO2 can still be opened");
        }
    }

    #[test]
    fn closing_game_with_mo2_open_offers_show_mo2() {
        let all = installs();
        let running = [mo2(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe")];
        let c = card_with_game(&all[0], &running, &all, GameState::Closing { pid: 9 });
        assert_eq!(c.open_label, "Show MO2");
        assert!(!c.can_play);
        // A different modlist's MO2 still blocks opening this one.
        let c = card_with_game(&all[1], &running, &all, GameState::Running { pid: 9 });
        assert!(!c.can_open);
    }
}
