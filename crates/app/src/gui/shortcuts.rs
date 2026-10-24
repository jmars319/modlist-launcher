//! Which modlists have a desktop or Start menu shortcut, for the cards.
//! A folder is read again only when its modified time changes.

use std::time::SystemTime;

use modlist_core::config::{Config, Installation};

use crate::platform::shell::{self, ShortcutPlace};

#[derive(Default)]
struct Folder {
    stamp: Option<SystemTime>,
    /// What each launcher shortcut names after `--launch`: an id or a name,
    /// resolved when asked so a renamed modlist still matches.
    targets: Vec<String>,
}

impl Folder {
    fn refresh(&mut self, place: ShortcutPlace, force: bool) {
        let stamp = shell::folder(place).and_then(|d| std::fs::metadata(d).ok()).and_then(|m| m.modified().ok());
        if force || stamp != self.stamp {
            self.stamp = stamp;
            self.targets = shell::launcher_shortcuts(place);
        }
    }

    fn has(&self, config: &Config, inst: &Installation) -> bool {
        self.targets.iter().any(|t| config.resolve(t).is_some_and(|i| i.id == inst.id))
    }
}

#[derive(Default)]
pub struct ShortcutIndex {
    desktop: Folder,
    start_menu: Folder,
}

impl ShortcutIndex {
    pub fn refresh(&mut self, force: bool) {
        self.desktop.refresh(ShortcutPlace::Desktop, force);
        self.start_menu.refresh(ShortcutPlace::StartMenu, force);
    }

    pub fn on_desktop(&self, config: &Config, inst: &Installation) -> bool {
        self.desktop.has(config, inst)
    }

    pub fn in_start_menu(&self, config: &Config, inst: &Installation) -> bool {
        self.start_menu.has(config, inst)
    }
}
