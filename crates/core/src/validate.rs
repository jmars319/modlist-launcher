//! Checks an installation before launch and explains anything that is wrong.

use crate::config::{Installation, PlayAction};
use crate::mo2::{self, Mo2Install};
use crate::paths::{file_name_eq, same_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
}

impl Issue {
    fn error(message: impl Into<String>) -> Self {
        Self { severity: Severity::Error, message: message.into() }
    }
    fn warning(message: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, message: message.into() }
    }
}

#[derive(Debug)]
pub struct Inspection {
    pub install: Option<Mo2Install>,
    pub issues: Vec<Issue>,
}

impl Inspection {
    pub fn can_open(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error && !i.message.starts_with(PLAY_PREFIX))
    }

    pub fn can_play(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn first_error(&self) -> Option<&Issue> {
        self.issues.iter().find(|i| i.severity == Severity::Error)
    }
}

/// Errors that only block Play, not opening MO2 itself.
const PLAY_PREFIX: &str = "Play: ";

pub fn inspect(inst: &Installation) -> Inspection {
    let mut issues = Vec::new();
    let exe = &inst.exe_path;

    if !file_name_eq(exe, mo2::MO2_EXE) {
        issues.push(Issue::error(format!("{} is not ModOrganizer.exe.", exe.display())));
        return Inspection { install: None, issues };
    }
    if !exe.exists() {
        let folder = exe.parent().filter(|p| p.is_dir());
        issues.push(Issue::error(match folder {
            Some(f) => format!("ModOrganizer.exe is missing from {}.", f.display()),
            None => format!("The folder {} no longer exists.", exe.parent().unwrap_or(exe).display()),
        }));
        return Inspection { install: None, issues };
    }
    if !inst.working_directory.is_dir() {
        issues
            .push(Issue::error(format!("The working directory {} does not exist.", inst.working_directory.display())));
    } else if exe.parent().is_some_and(|p| !same_path(p, &inst.working_directory)) {
        issues.push(Issue::warning(
            "The working directory is not the installation folder. Portable MO2 expects its own folder.",
        ));
    }

    let install = match mo2::read_install(exe) {
        Ok(i) => i,
        Err(e) => {
            issues.push(Issue::error(format!("Could not read the installation: {e}.")));
            return Inspection { install: None, issues };
        }
    };

    if !install.portable {
        issues.push(Issue::warning(
            "No ModOrganizer.ini next to the exe, so this is not a portable install. MO2 may show its instance picker.",
        ));
    }
    if let Some(game) = &install.game_path
        && !game.is_dir()
    {
        issues.push(Issue::warning(format!("The game folder {} was not found.", game.display())));
    }
    if let Some(profile) = &inst.profile
        && !install.profiles.iter().any(|p| p == profile)
    {
        issues.push(Issue::warning(format!("Profile \"{profile}\" no longer exists; MO2 will use its own selection.")));
    }
    if let PlayAction::Executable { title } = &inst.play_action {
        match install.executable(title) {
            None => issues.push(Issue::error(format!(
                "{PLAY_PREFIX}\"{title}\" is no longer one of this installation's MO2 executables."
            ))),
            Some(e) if !e.binary.is_file() => {
                issues.push(Issue::error(format!("{PLAY_PREFIX}{} was not found.", e.binary.display())))
            }
            Some(_) => {}
        }
    }
    Inspection { install: Some(install), issues }
}

/// Removes the internal prefix that marks Play-only errors.
pub fn display_message(issue: &Issue) -> &str {
    issue.message.strip_prefix(PLAY_PREFIX).unwrap_or(&issue.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mo2::fixtures::{ini_for, portable};

    fn setup() -> (tempfile::TempDir, Installation) {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        let bin = game.join("bin/game.exe");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"").unwrap();
        let fake = portable(tmp.path(), "List", &ini_for("Game", &game, &[("Game", &bin)]), &["Default", "Custom"]);
        let mut inst = Installation::new("List", fake.exe);
        inst.play_action = PlayAction::Executable { title: "Game".into() };
        (tmp, inst)
    }

    #[test]
    fn healthy_install_has_no_issues() {
        let (_t, inst) = setup();
        let r = inspect(&inst);
        assert!(r.issues.is_empty(), "{:?}", r.issues);
        assert!(r.can_play() && r.can_open());
    }

    #[test]
    fn missing_exe_and_missing_folder() {
        let (t, mut inst) = setup();
        inst.exe_path = t.path().join("List/sub/ModOrganizer.exe");
        let r = inspect(&inst);
        assert!(!r.can_open());
        assert!(r.issues[0].message.contains("no longer exists"));

        std::fs::remove_file(t.path().join("List/ModOrganizer.exe")).unwrap();
        inst.exe_path = t.path().join("List/ModOrganizer.exe");
        let r = inspect(&inst);
        assert!(r.issues[0].message.contains("ModOrganizer.exe is missing"));
    }

    #[test]
    fn wrong_exe_name() {
        let (t, mut inst) = setup();
        inst.exe_path = t.path().join("List/helper.exe");
        assert!(!inspect(&inst).can_open());
    }

    #[test]
    fn missing_play_executable_blocks_only_play() {
        let (t, inst) = setup();
        std::fs::remove_file(t.path().join("Game/bin/game.exe")).unwrap();
        let r = inspect(&inst);
        assert!(!r.can_play());
        assert!(r.can_open());
        assert!(display_message(r.first_error().unwrap()).ends_with("was not found."));

        let mut inst = inst;
        inst.play_action = PlayAction::Executable { title: "Gone".into() };
        let r = inspect(&inst);
        assert!(!r.can_play() && r.can_open());
    }

    #[test]
    fn warnings_do_not_block() {
        let (t, mut inst) = setup();
        inst.profile = Some("Removed".into());
        inst.working_directory = t.path().to_path_buf();
        let r = inspect(&inst);
        assert_eq!(r.issues.len(), 2);
        assert!(r.issues.iter().all(|i| i.severity == Severity::Warning));
        assert!(r.can_play());
    }

    #[test]
    fn missing_working_directory_is_an_error() {
        let (t, mut inst) = setup();
        inst.working_directory = t.path().join("nowhere");
        assert!(!inspect(&inst).can_open());
    }
}
