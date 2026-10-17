//! Command-line parsing for shortcuts and Steam launch options.

use std::ffi::OsString;

use crate::launch::LaunchMode;

pub const USAGE: &str = "\
Usage:
  modlist-launcher                         Open the launcher
  modlist-launcher --launch <id|name>      Run an installation's Play action
  modlist-launcher --open <id|name>        Open an installation's Mod Organizer
  modlist-launcher --steam <id> -- <cmd>   Used by Steam launch options (%command%)
  modlist-launcher --help | --version";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Gui,
    Launch {
        target: String,
        mode: LaunchMode,
    },
    /// Started by Steam. `original` is Steam's own `%command%`, kept so the
    /// user can still choose to start the game unmodded.
    Steam {
        target: String,
        original: Vec<String>,
    },
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("{0} needs a value")]
    MissingValue(&'static str),
    #[error("unknown argument: {0}")]
    Unknown(String),
}

pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Command, CliError> {
    let mut it = args.into_iter().skip(1).map(|a| a.to_string_lossy().into_owned());
    let Some(first) = it.next() else { return Ok(Command::Gui) };
    let mut value = |flag: &'static str| it.next().filter(|v| !v.is_empty()).ok_or(CliError::MissingValue(flag));
    let cmd = match first.as_str() {
        "--launch" => Command::Launch { target: value("--launch")?, mode: LaunchMode::Play },
        "--open" => Command::Launch { target: value("--open")?, mode: LaunchMode::OpenMo2 },
        "--steam" => {
            let target = value("--steam")?;
            let rest: Vec<String> = it.collect();
            let original = match rest.first().map(String::as_str) {
                Some("--") => rest[1..].to_vec(),
                _ => rest,
            };
            return Ok(Command::Steam { target, original });
        }
        "--help" | "-h" | "/?" => Command::Help,
        "--version" | "-V" => Command::Version,
        other => return Err(CliError::Unknown(other.to_string())),
    };
    match it.next() {
        Some(extra) => Err(CliError::Unknown(extra)),
        None => Ok(cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, CliError> {
        parse(std::iter::once("launcher.exe").chain(args.iter().copied()).map(OsString::from))
    }

    #[test]
    fn no_args_opens_gui() {
        assert_eq!(p(&[]), Ok(Command::Gui));
    }

    #[test]
    fn launch_and_open() {
        assert_eq!(
            p(&["--launch", "Witcher 3 Ascended"]),
            Ok(Command::Launch { target: "Witcher 3 Ascended".into(), mode: LaunchMode::Play })
        );
        assert_eq!(p(&["--open", "abc"]), Ok(Command::Launch { target: "abc".into(), mode: LaunchMode::OpenMo2 }));
        assert_eq!(p(&["--launch"]), Err(CliError::MissingValue("--launch")));
        assert_eq!(p(&["--launch", "a", "b"]), Err(CliError::Unknown("b".into())));
    }

    #[test]
    fn steam_keeps_original_command() {
        assert_eq!(
            p(&["--steam", "id", "--", r"C:\Games\Cyberpunk 2077\REDprelauncher.exe", "-skipStartScreen"]),
            Ok(Command::Steam {
                target: "id".into(),
                original: vec![r"C:\Games\Cyberpunk 2077\REDprelauncher.exe".into(), "-skipStartScreen".into()]
            })
        );
        assert_eq!(p(&["--steam", "id"]), Ok(Command::Steam { target: "id".into(), original: vec![] }));
    }

    #[test]
    fn rejects_unknown() {
        assert_eq!(p(&["--frobnicate"]), Err(CliError::Unknown("--frobnicate".into())));
        assert_eq!(p(&["--help"]), Ok(Command::Help));
    }
}
