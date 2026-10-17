//! Read-only inspection of a Mod Organizer 2 installation.

use std::path::{Path, PathBuf};

use crate::ini::Ini;
use crate::paths::file_name_eq;

pub const MO2_EXE: &str = "ModOrganizer.exe";
pub const MO2_INI: &str = "ModOrganizer.ini";
pub const PORTABLE_LOCK: &str = "portable.txt";

#[derive(Debug, Clone, PartialEq)]
pub struct Mo2Executable {
    pub title: String,
    pub binary: PathBuf,
    pub arguments: String,
    pub working_directory: Option<PathBuf>,
    pub hidden: bool,
    pub steam_app_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mo2Install {
    pub root: PathBuf,
    pub exe: PathBuf,
    /// `ModOrganizer.ini` sits next to the exe, so MO2 starts this folder as
    /// its portable instance.
    pub portable: bool,
    /// `portable.txt` forces portable mode and disables instance switching.
    pub portable_lock: bool,
    pub version: Option<String>,
    pub game_name: Option<String>,
    pub game_path: Option<PathBuf>,
    pub selected_profile: Option<String>,
    pub executables: Vec<Mo2Executable>,
    pub profiles_dir: PathBuf,
    pub profiles: Vec<String>,
    /// Display name from a Wabbajack `*.compiler_settings` file, if present.
    pub modlist_name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Mo2Error {
    #[error("{0} was not found")]
    ExeMissing(PathBuf),
    #[error("{0} is not ModOrganizer.exe")]
    NotMo2(PathBuf),
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
}

impl Mo2Install {
    pub fn executable(&self, title: &str) -> Option<&Mo2Executable> {
        self.executables
            .iter()
            .find(|e| e.title == title)
            .or_else(|| self.executables.iter().find(|e| e.title.eq_ignore_ascii_case(title)))
    }

    /// Picks the executable that most likely starts the game itself, for the
    /// default Play action of a newly added installation.
    pub fn default_play_executable(&self) -> Option<&Mo2Executable> {
        let visible = || self.executables.iter().filter(|e| !e.hidden);
        if let Some(name) = &self.game_name
            && let Some(e) = visible().find(|e| e.title.eq_ignore_ascii_case(name))
        {
            return Some(e);
        }
        let is_tool = |e: &Mo2Executable| {
            let t = e.title.to_lowercase();
            ["explore", "loot", "xedit", "deploy", "conflict", "creation kit", "bodyslide", "nemesis", "fnis"]
                .iter()
                .any(|k| t.contains(k))
        };
        if let Some(game) = &self.game_path
            && let Some(e) = visible()
                .find(|e| !is_tool(e) && crate::paths::normalize(&e.binary).starts_with(&crate::paths::normalize(game)))
        {
            return Some(e);
        }
        visible().find(|e| !is_tool(e))
    }
}

/// Reads an installation from the path to its `ModOrganizer.exe`.
pub fn read_install(exe: &Path) -> Result<Mo2Install, Mo2Error> {
    if !file_name_eq(exe, MO2_EXE) {
        return Err(Mo2Error::NotMo2(exe.to_path_buf()));
    }
    if !exe.is_file() {
        return Err(Mo2Error::ExeMissing(exe.to_path_buf()));
    }
    let root = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let ini_path = root.join(MO2_INI);
    let portable = ini_path.is_file();
    let portable_lock = root.join(PORTABLE_LOCK).is_file();
    let ini = if portable {
        let text = std::fs::read(&ini_path).map_err(|source| Mo2Error::Io { path: ini_path.clone(), source })?;
        Ini::parse(&String::from_utf8_lossy(&text))
    } else {
        Ini::default()
    };

    let non_empty = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let base_dir = non_empty(ini.get("Settings", "base_directory")).map(PathBuf::from).unwrap_or_else(|| root.clone());
    let expand = |v: String| -> PathBuf {
        let replaced = v.replace("%BASE_DIR%", &base_dir.to_string_lossy());
        let p = PathBuf::from(replaced);
        if p.is_absolute() { p } else { base_dir.join(p) }
    };
    let profiles_dir =
        non_empty(ini.get("Settings", "profiles_directory")).map(expand).unwrap_or_else(|| base_dir.join("profiles"));

    Ok(Mo2Install {
        version: non_empty(ini.get("General", "version")),
        game_name: non_empty(ini.get("General", "gameName")),
        game_path: non_empty(ini.get("General", "gamePath")).map(PathBuf::from),
        selected_profile: non_empty(ini.get("General", "selected_profile")),
        executables: read_executables(&ini),
        profiles: list_profiles(&profiles_dir),
        modlist_name: find_modlist_name(&root),
        profiles_dir,
        root,
        exe: exe.to_path_buf(),
        portable,
        portable_lock,
    })
}

fn read_executables(ini: &Ini) -> Vec<Mo2Executable> {
    let size: usize = ini.get("customExecutables", "size").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    (1..=size.min(512))
        .filter_map(|i| {
            let get = |k: &str| ini.get("customExecutables", &format!("{i}\\{k}")).map(str::to_string);
            let title = get("title").filter(|t| !t.trim().is_empty())?;
            let binary = PathBuf::from(get("binary").filter(|b| !b.trim().is_empty())?);
            Some(Mo2Executable {
                title,
                binary,
                arguments: get("arguments").unwrap_or_default(),
                working_directory: get("workingDirectory").filter(|w| !w.trim().is_empty()).map(PathBuf::from),
                hidden: get("hide").is_some_and(|h| h.eq_ignore_ascii_case("true")),
                steam_app_id: get("steamAppID").and_then(|s| s.trim().parse().ok()).filter(|&id| id != 0),
            })
        })
        .collect()
}

fn list_profiles(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn find_modlist_name(root: &Path) -> Option<String> {
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            name.strip_suffix(".compiler_settings").map(|s| s.trim().to_string())
        })
        .find(|s| !s.is_empty())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    pub struct FakeInstall {
        pub root: PathBuf,
        pub exe: PathBuf,
    }

    /// Creates a fake portable MO2 install. `ModOrganizer.exe` is an empty
    /// file: tests only ever read it, never run it.
    pub fn portable(parent: &Path, folder: &str, ini: &str, profiles: &[&str]) -> FakeInstall {
        let root = parent.join(folder);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("ModOrganizer.exe"), b"").unwrap();
        std::fs::write(root.join("ModOrganizer.ini"), ini).unwrap();
        std::fs::write(root.join("portable.txt"), "Created by Wabbajack").unwrap();
        for p in profiles {
            std::fs::create_dir_all(root.join("profiles").join(p)).unwrap();
        }
        FakeInstall { exe: root.join("ModOrganizer.exe"), root }
    }

    /// Every path under `dir`, with each file's contents and modified time.
    /// Uses `fs::metadata` rather than directory-entry metadata, which NTFS
    /// updates lazily and which made this flaky. Directory timestamps are
    /// left out because they track their children.
    pub fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>, Option<std::time::SystemTime>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let path = e.path();
                let m = std::fs::metadata(&path).unwrap();
                if m.is_dir() {
                    out.push((path.clone(), Vec::new(), None));
                    stack.push(path);
                } else {
                    out.push((path.clone(), std::fs::read(&path).unwrap(), m.modified().ok()));
                }
            }
        }
        out.sort();
        out
    }

    pub fn ini_for(game_name: &str, game_path: &Path, exes: &[(&str, &Path)]) -> String {
        let esc = |p: &Path| p.to_string_lossy().replace('\\', "\\\\");
        let mut s = format!(
            "[General]\ngameName={game_name}\ngamePath=@ByteArray({})\nselected_profile=@ByteArray(Default)\nversion=2.5.2\n\n[customExecutables]\nsize={}\n",
            esc(game_path),
            exes.len()
        );
        for (i, (title, bin)) in exes.iter().enumerate() {
            let n = i + 1;
            s.push_str(&format!(
                "{n}\\title={title}\n{n}\\binary={}\n{n}\\arguments=\n{n}\\hide=false\n",
                bin.to_string_lossy().replace('\\', "/")
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn reads_portable_install() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        let ini = ini_for(
            "Cyberpunk 2077",
            &game,
            &[
                ("Cyberpunk 2077", &game.join("bin/x64/Cyberpunk2077.exe")),
                ("Explore Virtual Folder", &tmp.path().join("explorer++/Explorer++.exe")),
            ],
        );
        let fake = portable(tmp.path(), "Ultrapunk", &ini, &["Default", "5. Custom", "1. Performance"]);
        std::fs::write(fake.root.join("Project Ultrapunk.compiler_settings"), "{}").unwrap();

        let install = read_install(&fake.exe).unwrap();
        assert!(install.portable);
        assert!(install.portable_lock);
        assert_eq!(install.game_name.as_deref(), Some("Cyberpunk 2077"));
        assert_eq!(install.game_path.as_deref(), Some(game.as_path()));
        assert_eq!(install.selected_profile.as_deref(), Some("Default"));
        assert_eq!(install.version.as_deref(), Some("2.5.2"));
        assert_eq!(install.profiles, vec!["1. Performance", "5. Custom", "Default"]);
        assert_eq!(install.executables.len(), 2);
        assert_eq!(install.modlist_name.as_deref(), Some("Project Ultrapunk"));
        assert_eq!(install.default_play_executable().unwrap().title, "Cyberpunk 2077");
        assert_eq!(install.executable("cyberpunk 2077").unwrap().title, "Cyberpunk 2077");
    }

    #[test]
    fn default_play_skips_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("W3");
        let ini = ini_for(
            "The Witcher 3: Wild Hunt",
            &game,
            &[("Explore Virtual Folder", &tmp.path().join("e.exe")), ("Witcher 3", &game.join("bin/witcher3.exe"))],
        );
        let fake = portable(tmp.path(), "W3A", &ini, &["Default"]);
        let install = read_install(&fake.exe).unwrap();
        assert_eq!(install.default_play_executable().unwrap().title, "Witcher 3");
    }

    #[test]
    fn non_portable_and_missing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("ModOrganizer.exe"), b"").unwrap();
        let install = read_install(&tmp.path().join("ModOrganizer.exe")).unwrap();
        assert!(!install.portable);
        assert!(install.executables.is_empty());

        assert!(matches!(read_install(&tmp.path().join("nope/ModOrganizer.exe")), Err(Mo2Error::ExeMissing(_))));
        assert!(matches!(read_install(&tmp.path().join("helper.exe")), Err(Mo2Error::NotMo2(_))));
    }

    #[test]
    fn honors_custom_profiles_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = portable(tmp.path(), "L", "[Settings]\nprofiles_directory=%BASE_DIR%/alt_profiles\n", &[]);
        std::fs::create_dir_all(fake.root.join("alt_profiles/Main")).unwrap();
        let install = read_install(&fake.exe).unwrap();
        assert_eq!(install.profiles, vec!["Main"]);
    }

    #[test]
    fn reading_never_modifies_the_install() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = portable(tmp.path(), "L", "[General]\ngameName=X\n", &["Default"]);
        let before = snapshot(&fake.root);
        read_install(&fake.exe).unwrap();
        assert_eq!(before, snapshot(&fake.root));
    }
}
