//! Launcher configuration, stored only in the launcher's own data folder.

use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub installations: Vec<Installation>,
    #[serde(default)]
    pub scan_roots: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            settings: Settings::default(),
            installations: Vec::new(),
            scan_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub appearance: Appearance,
    pub close_after_launch: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: Uuid,
    pub name: String,
    pub exe_path: PathBuf,
    pub working_directory: PathBuf,
    #[serde(default)]
    pub play_action: PlayAction,
    /// MO2 profile passed with `-p`; `None` keeps MO2's own selection.
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub steam_app_id: Option<u32>,
    #[serde(default)]
    pub artwork: Artwork,
    /// Present while this installation is wired to a Steam Play button, so the
    /// previous launch options can be restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_hook: Option<SteamHook>,
}

impl Installation {
    pub fn new(name: impl Into<String>, exe_path: PathBuf) -> Self {
        let working_directory = exe_path.parent().map(Path::to_path_buf).unwrap_or_default();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            exe_path,
            working_directory,
            play_action: PlayAction::OpenMo2,
            profile: None,
            steam_app_id: None,
            artwork: Artwork::Automatic,
            steam_hook: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayAction {
    /// Open Mod Organizer's main window.
    #[default]
    OpenMo2,
    /// Run one of the installation's configured MO2 executables directly.
    Executable { title: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Artwork {
    /// Steam's local library images, then the game's icon.
    #[default]
    Automatic,
    /// An image copied into the launcher's data folder (relative path).
    Custom {
        file: PathBuf,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamHook {
    pub account_id: u32,
    pub app_id: u32,
    pub previous_launch_options: String,
    pub applied_launch_options: String,
}

impl Config {
    pub fn find(&self, id: Uuid) -> Option<&Installation> {
        self.installations.iter().find(|i| i.id == id)
    }

    pub fn find_mut(&mut self, id: Uuid) -> Option<&mut Installation> {
        self.installations.iter_mut().find(|i| i.id == id)
    }

    /// Resolves a command-line target: an id, or a name (case-insensitive).
    pub fn resolve(&self, target: &str) -> Option<&Installation> {
        if let Ok(id) = Uuid::parse_str(target) {
            return self.find(id);
        }
        self.installations.iter().find(|i| i.name.eq_ignore_ascii_case(target.trim()))
    }

    pub fn contains_exe(&self, exe: &Path) -> bool {
        self.installations.iter().any(|i| crate::paths::same_path(&i.exe_path, exe))
    }

    pub fn remove(&mut self, id: Uuid) -> Option<Installation> {
        let pos = self.installations.iter().position(|i| i.id == id)?;
        Some(self.installations.remove(pos))
    }

    /// Moves an installation by `delta` places, clamped to the list bounds.
    pub fn move_by(&mut self, id: Uuid, delta: isize) -> bool {
        let Some(from) = self.installations.iter().position(|i| i.id == id) else { return false };
        let to = (from as isize + delta).clamp(0, self.installations.len() as isize - 1) as usize;
        if from == to {
            return false;
        }
        let item = self.installations.remove(from);
        self.installations.insert(to, item);
        true
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("refusing to write outside the launcher data folder: {0}")]
    OutsideDataDir(PathBuf),
    #[error(
        "the configuration was written by a newer version of Modlist Launcher (schema {0}); this version will not overwrite it"
    )]
    NewerSchema(u32),
    #[error("{action} {path}: {source}")]
    Io { action: &'static str, path: PathBuf, source: std::io::Error },
    #[error("could not encode the configuration: {0}")]
    Encode(#[from] serde_json::Error),
}

fn io<'a>(action: &'static str, path: &'a Path) -> impl FnOnce(std::io::Error) -> ConfigError + 'a {
    move |source| ConfigError::Io { action, path: path.to_path_buf(), source }
}

/// The launcher's data folder. Every launcher-owned write goes through here,
/// which keeps writes away from MO2 installations by construction.
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolves a relative path inside the data folder, rejecting absolute
    /// paths and `..` components.
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, ConfigError> {
        let safe = relative.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !safe || relative.as_os_str().is_empty() {
            return Err(ConfigError::OutsideDataDir(relative.to_path_buf()));
        }
        Ok(self.root.join(relative))
    }

    /// Writes a file atomically: temp file, flush, then rename over the target.
    pub fn write_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<PathBuf, ConfigError> {
        let target = self.resolve(relative)?;
        let parent = target.parent().unwrap_or(&self.root).to_path_buf();
        std::fs::create_dir_all(&parent).map_err(io("create", &parent))?;
        let tmp = target.with_extension(format!("tmp-{}", std::process::id()));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp).map_err(io("create", &tmp))?;
            f.write_all(bytes).map_err(io("write", &tmp))?;
            f.sync_all().map_err(io("flush", &tmp))?;
        }
        std::fs::rename(&tmp, &target).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            ConfigError::Io { action: "replace", path: target.clone(), source: e }
        })?;
        Ok(target)
    }

    /// Appends a line, rotating the file to `<name>.old` past `max_bytes`.
    pub fn append_line(&self, relative: &Path, line: &str, max_bytes: u64) -> Result<(), ConfigError> {
        use std::io::Write;
        let target = self.resolve(relative)?;
        let parent = target.parent().unwrap_or(&self.root).to_path_buf();
        std::fs::create_dir_all(&parent).map_err(io("create", &parent))?;
        if std::fs::metadata(&target).is_ok_and(|m| m.len() > max_bytes) {
            let _ = std::fs::rename(&target, target.with_extension("old"));
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&target).map_err(io("open", &target))?;
        writeln!(f, "{line}").map_err(io("write", &target))
    }

    pub fn copy_in(&self, source: &Path, relative: &Path) -> Result<PathBuf, ConfigError> {
        let bytes = std::fs::read(source).map_err(io("read", source))?;
        self.write_atomic(relative, &bytes)
    }
}

pub fn timestamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    /// Problems the user should hear about, such as a corrupt file that was
    /// set aside.
    pub warnings: Vec<String>,
    /// False when saving would destroy data this version does not understand.
    pub writable: bool,
}

pub struct ConfigStore {
    data: DataDir,
}

impl ConfigStore {
    pub fn new(data: DataDir) -> Self {
        Self { data }
    }

    pub fn data(&self) -> &DataDir {
        &self.data
    }

    pub fn path(&self) -> PathBuf {
        self.data.root().join(CONFIG_FILE)
    }

    pub fn load(&self) -> Loaded {
        let path = self.path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Loaded { config: Config::default(), warnings: Vec::new(), writable: true };
            }
            Err(e) => {
                return Loaded {
                    config: Config::default(),
                    warnings: vec![format!(
                        "Could not read {}: {e}. Changes will not be saved until this is fixed.",
                        path.display()
                    )],
                    writable: false,
                };
            }
        };
        let probe: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
        if let Ok(v) = &probe {
            let version = v.get("schemaVersion").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            if version > SCHEMA_VERSION {
                return Loaded {
                    config: serde_json::from_value(v.clone()).unwrap_or_default(),
                    warnings: vec![ConfigError::NewerSchema(version).to_string()],
                    writable: false,
                };
            }
        }
        match probe.and_then(serde_json::from_value::<Config>) {
            Ok(mut config) => {
                config.schema_version = SCHEMA_VERSION;
                Loaded { config, warnings: Vec::new(), writable: true }
            }
            Err(e) => {
                let aside = self.data.root().join(format!("config.corrupt-{}.json", timestamp()));
                let moved = std::fs::rename(&path, &aside).is_ok();
                let mut msg = format!("The launcher configuration could not be read ({e}).");
                if moved {
                    msg.push_str(&format!(" It was set aside as {} and an empty list was started.", aside.display()));
                }
                Loaded { config: Config::default(), warnings: vec![msg], writable: moved }
            }
        }
    }

    /// Saves atomically, keeping the previous file as `config.json.bak`.
    pub fn save(&self, config: &Config) -> Result<(), ConfigError> {
        let mut json = serde_json::to_vec_pretty(config)?;
        json.push(b'\n');
        let path = self.path();
        if path.is_file() {
            let bak = self.data.resolve(Path::new("config.json.bak"))?;
            std::fs::copy(&path, &bak).map_err(io("back up", &path))?;
        }
        self.data.write_atomic(Path::new(CONFIG_FILE), &json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ConfigStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(DataDir::new(tmp.path().join("data")));
        (tmp, store)
    }

    fn sample() -> Config {
        let mut a =
            Installation::new("Project Ultrapunk", PathBuf::from(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.exe"));
        a.play_action = PlayAction::Executable { title: "Cyberpunk 2077".into() };
        a.steam_app_id = Some(1091500);
        let b = Installation::new(
            "Witcher 3 Ascended",
            PathBuf::from(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe"),
        );
        Config { installations: vec![a, b], ..Config::default() }
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_t, store) = store();
        let loaded = store.load();
        assert!(loaded.config.installations.is_empty());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.writable);
    }

    #[test]
    fn round_trip_and_backup() {
        let (_t, store) = store();
        let mut cfg = sample();
        store.save(&cfg).unwrap();
        assert_eq!(store.load().config, cfg);
        assert!(!store.data().root().join("config.json.bak").exists());

        cfg.installations[0].name = "Renamed".into();
        store.save(&cfg).unwrap();
        assert_eq!(store.load().config.installations[0].name, "Renamed");
        let bak = std::fs::read_to_string(store.data().root().join("config.json.bak")).unwrap();
        assert!(bak.contains("Project Ultrapunk"));
        let leftovers: Vec<_> = std::fs::read_dir(store.data().root())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn working_directory_defaults_to_exe_folder() {
        let i =
            Installation::new("x", PathBuf::from(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe"));
        assert_eq!(i.working_directory, PathBuf::from(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended"));
    }

    #[test]
    fn corrupt_file_is_set_aside() {
        let (_t, store) = store();
        std::fs::create_dir_all(store.data().root()).unwrap();
        std::fs::write(store.path(), b"{ not json").unwrap();
        let loaded = store.load();
        assert!(loaded.config.installations.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.writable);
        let aside: Vec<_> = std::fs::read_dir(store.data().root())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("config.corrupt-"))
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(std::fs::read(aside[0].path()).unwrap(), b"{ not json");
    }

    #[test]
    fn newer_schema_is_not_overwritten() {
        let (_t, store) = store();
        std::fs::create_dir_all(store.data().root()).unwrap();
        std::fs::write(store.path(), br#"{"schemaVersion": 99, "installations": []}"#).unwrap();
        let loaded = store.load();
        assert!(!loaded.writable);
        assert!(loaded.warnings[0].contains("newer version"));
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let (_t, store) = store();
        std::fs::create_dir_all(store.data().root()).unwrap();
        std::fs::write(
            store.path(),
            br#"{"schemaVersion":1,"installations":[{"id":"6f1c2d36-6a4c-4b8e-9a51-0c6a8f2b9e10","name":"A","exePath":"C:\\A\\ModOrganizer.exe","workingDirectory":"C:\\A"}]}"#,
        )
        .unwrap();
        let loaded = store.load();
        assert!(loaded.warnings.is_empty());
        let i = &loaded.config.installations[0];
        assert_eq!(i.play_action, PlayAction::OpenMo2);
        assert_eq!(i.artwork, Artwork::Automatic);
    }

    #[test]
    fn data_dir_rejects_escapes() {
        let d = DataDir::new(r"C:\data");
        assert!(d.resolve(Path::new("../x")).is_err());
        assert!(d.resolve(Path::new(r"C:\Modlists\ProjectUltrapunk\ModOrganizer.ini")).is_err());
        assert!(d.resolve(Path::new(r"\x")).is_err());
        assert!(d.resolve(Path::new("")).is_err());
        assert_eq!(d.resolve(Path::new("artwork/a.png")).unwrap(), Path::new(r"C:\data").join("artwork/a.png"));
    }

    #[test]
    fn append_rotates() {
        let tmp = tempfile::tempdir().unwrap();
        let d = DataDir::new(tmp.path());
        for i in 0..50 {
            d.append_line(Path::new("launcher.log"), &format!("line {i}"), 100).unwrap();
        }
        assert!(tmp.path().join("launcher.old").exists());
        assert!(std::fs::metadata(tmp.path().join("launcher.log")).unwrap().len() <= 120);
        assert!(d.append_line(Path::new("../escape.log"), "x", 100).is_err());
    }

    #[test]
    fn resolve_reorder_remove() {
        let mut cfg = sample();
        let a = cfg.installations[0].id;
        let b = cfg.installations[1].id;
        assert_eq!(cfg.resolve("witcher 3 ascended").unwrap().id, b);
        assert_eq!(cfg.resolve(&a.to_string()).unwrap().id, a);
        assert!(cfg.resolve("nope").is_none());
        assert!(cfg.move_by(a, 1));
        assert_eq!(cfg.installations[1].id, a);
        assert!(!cfg.move_by(a, 5));
        assert!(cfg.move_by(a, -9));
        assert_eq!(cfg.installations[0].id, a);
        assert!(cfg.contains_exe(Path::new(r"c:/modlists/projectultrapunk/modorganizer.exe")));
        assert_eq!(cfg.remove(a).unwrap().id, a);
        assert_eq!(cfg.installations.len(), 1);
    }
}
