//! Guessing which Steam app a modlist plays, for its Steam artwork and its
//! Steam Play button.

use super::library::AppInfo;
use crate::mo2::Mo2Install;
use crate::paths::normalize;

/// Letters and digits only, lowercase: "The Witcher 3: Wild Hunt" and
/// "the witcher 3 wild hunt" compare equal.
fn squash(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// The Steam app a modlist plays, in order of confidence:
/// 1. the Steam ID on the executable the Play button runs;
/// 2. the Steam ID on the executable named after the game;
/// 3. the installed Steam app whose folder is MO2's game folder;
/// 4. the installed Steam app whose name matches MO2's game name, exactly
///    first, then the shortest name containing it.
///
/// Steam IDs on other executables are never used: modlists list tools such
/// as the Creation Kit's FaceGen Generator, whose ID is not the game's.
pub fn guess_app_id(install: &Mo2Install, play_title: Option<&str>, installed: &[AppInfo]) -> Option<u32> {
    if let Some(id) = play_title.and_then(|t| install.executable(t)).and_then(|e| e.steam_app_id) {
        return Some(id);
    }
    let game = install.game_name.as_deref();
    if let Some(id) = game.and_then(|g| install.executable(g)).and_then(|e| e.steam_app_id) {
        return Some(id);
    }
    if let Some(path) = &install.game_path {
        let target = normalize(path);
        if let Some(app) = installed.iter().find(|a| normalize(&a.install_dir) == target) {
            return Some(app.app_id);
        }
    }
    let wanted = squash(game?);
    if wanted.is_empty() {
        return None;
    }
    installed
        .iter()
        .find(|a| squash(&a.name) == wanted)
        .or_else(|| installed.iter().filter(|a| squash(&a.name).contains(&wanted)).min_by_key(|a| a.name.len()))
        .map(|a| a.app_id)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::mo2::Mo2Executable;

    fn exe(title: &str, steam: Option<u32>) -> Mo2Executable {
        Mo2Executable {
            title: title.into(),
            binary: PathBuf::from(format!(r"C:\x\{title}.exe")),
            arguments: String::new(),
            working_directory: None,
            hidden: false,
            steam_app_id: steam,
        }
    }

    fn install(game: &str, game_path: &str, exes: Vec<Mo2Executable>) -> Mo2Install {
        Mo2Install {
            root: PathBuf::from(r"C:\Modlists\L"),
            exe: PathBuf::from(r"C:\Modlists\L\ModOrganizer.exe"),
            portable: true,
            portable_lock: true,
            version: None,
            game_name: Some(game.into()),
            game_path: Some(PathBuf::from(game_path)),
            selected_profile: None,
            executables: exes,
            profiles_dir: PathBuf::new(),
            profiles: vec![],
            modlist_name: None,
        }
    }

    fn app(id: u32, name: &str, dir: &str) -> AppInfo {
        AppInfo { app_id: id, name: name.into(), install_dir: PathBuf::from(dir) }
    }

    fn library() -> Vec<AppInfo> {
        vec![
            app(1946160, "Fallout 4: Creation Kit", r"C:\Steam\steamapps\common\Fallout 4 Creation Kit"),
            app(377160, "Fallout 4", r"C:\Steam\steamapps\common\Fallout 4"),
            app(
                489830,
                "The Elder Scrolls V: Skyrim Special Edition",
                r"D:\Lib\steamapps\common\Skyrim Special Edition",
            ),
            app(292030, "The Witcher 3: Wild Hunt — Remastered", r"C:\Steam\steamapps\common\The Witcher 3"),
            app(1091500, "Cyberpunk 2077", r"C:\Steam\steamapps\common\Cyberpunk 2077"),
        ]
    }

    #[test]
    fn ignores_a_tools_steam_id() {
        // Life in the Ruins: a stock game folder, and FaceGen Generator carries
        // the Creation Kit's ID.
        let litr = install(
            "Fallout 4",
            r"C:\Modlists\LifeInTheRuins\Stock Game Folder",
            vec![exe("Fallout 4", None), exe("Play Life In The Ruins", None), exe("FaceGen Generator", Some(1946160))],
        );
        assert_eq!(guess_app_id(&litr, Some("Play Life In The Ruins"), &library()), Some(377160));
    }

    #[test]
    fn matches_stock_game_lists_by_name() {
        let frostbound =
            install("Skyrim Special Edition", r"C:\Modlists\Frostbound\stock game", vec![exe("Frostbound", None)]);
        assert_eq!(guess_app_id(&frostbound, Some("Frostbound"), &library()), Some(489830));
        let w3 = install("The Witcher 3: Wild Hunt", r"C:\Elsewhere", vec![]);
        assert_eq!(guess_app_id(&w3, None, &library()), Some(292030));
    }

    #[test]
    fn prefers_play_id_then_game_folder() {
        let with_id = install("Cyberpunk 2077", r"C:\x", vec![exe("Play", Some(1091500)), exe("Tool", Some(5))]);
        assert_eq!(guess_app_id(&with_id, Some("Play"), &[]), Some(1091500));
        let in_steam = install("Something Else", r"C:\Steam\steamapps\common\Cyberpunk 2077", vec![]);
        assert_eq!(guess_app_id(&in_steam, None, &library()), Some(1091500));
        let named = install("Fallout 4", r"C:\x", vec![exe("Fallout 4", Some(377160))]);
        assert_eq!(guess_app_id(&named, Some("Missing"), &[]), Some(377160));
    }

    #[test]
    fn unknown_game_has_no_guess() {
        let other = install("Some Indie Game", r"C:\x", vec![exe("Tool", Some(42))]);
        assert_eq!(guess_app_id(&other, None, &library()), None);
    }
}
