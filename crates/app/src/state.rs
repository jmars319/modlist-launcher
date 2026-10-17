//! Loaded configuration plus the derived, cached view of each installation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use modlist_core::config::{Config, ConfigStore, DataDir, Installation, timestamp};
use modlist_core::process::RunningProcess;
use modlist_core::steam::SteamRoot;
use modlist_core::validate::{self, Inspection};
use uuid::Uuid;

use crate::platform;

pub struct AppState {
    pub store: ConfigStore,
    pub config: Config,
    pub writable: bool,
    pub steam: Option<SteamRoot>,
    pub inspections: HashMap<Uuid, Inspection>,
}

impl AppState {
    pub fn load() -> (Self, Vec<String>) {
        let store = ConfigStore::new(DataDir::new(platform::data_dir()));
        let loaded = store.load();
        let mut state = Self {
            store,
            config: loaded.config,
            writable: loaded.writable,
            steam: platform::steam::locate(),
            inspections: HashMap::new(),
        };
        state.inspect_all();
        for w in &loaded.warnings {
            state.log(&format!("config: {w}"));
        }
        (state, loaded.warnings)
    }

    pub fn data(&self) -> &DataDir {
        self.store.data()
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self.writable {
            return Err("Changes were not saved because the configuration could not be read safely. See the launcher data folder.".into());
        }
        self.store.save(&self.config).map_err(|e| {
            self.log(&format!("save failed: {e}"));
            format!("Could not save the launcher configuration: {e}")
        })
    }

    pub fn inspect_all(&mut self) {
        self.inspections = self.config.installations.iter().map(|i| (i.id, validate::inspect(i))).collect();
    }

    pub fn inspect(&mut self, id: Uuid) {
        if let Some(inst) = self.config.find(id) {
            let insp = validate::inspect(inst);
            self.inspections.insert(id, insp);
        }
    }

    pub fn get(&self, id: Uuid) -> Option<(&Installation, Option<&Inspection>)> {
        let inst = self.config.find(id)?;
        Some((inst, self.inspections.get(&id)))
    }

    pub fn log(&self, line: &str) {
        let _ = self.data().append_line(Path::new("launcher.log"), &format!("[{}] {line}", timestamp()), 512 * 1024);
    }
}

pub fn running_mo2() -> Vec<RunningProcess> {
    platform::processes::running(|n| n.eq_ignore_ascii_case(modlist_core::mo2::MO2_EXE))
}

pub fn current_exe() -> PathBuf {
    std::env::current_exe()
        .map(modlist_core::paths::strip_verbatim)
        .unwrap_or_else(|_| PathBuf::from("modlist-launcher.exe"))
}

pub fn parse_id(s: &str) -> Option<Uuid> {
    Uuid::parse_str(s).ok()
}
