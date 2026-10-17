//! Builds the exact process to start for an installation.

use std::path::PathBuf;

use crate::config::{Installation, PlayAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Open Mod Organizer's main window.
    OpenMo2,
    /// Run the installation's Play action.
    Play,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
}

impl LaunchSpec {
    pub fn command_line(&self) -> String {
        std::iter::once(quote_arg(&self.program.to_string_lossy()))
            .chain(self.args.iter().map(|a| quote_arg(a)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn build(inst: &Installation, mode: LaunchMode) -> LaunchSpec {
    let mut args = Vec::new();
    if let Some(profile) = inst.profile.as_deref().filter(|p| !p.is_empty()) {
        args.push("-p".to_string());
        args.push(profile.to_string());
    }
    if let (LaunchMode::Play, PlayAction::Executable { title }) = (mode, &inst.play_action) {
        // An empty instance name targets the portable instance. MO2 splits on
        // the first colon only, so titles that contain colons are safe.
        args.push(format!("moshortcut://:{title}"));
    }
    LaunchSpec { program: inst.exe_path.clone(), args, working_directory: inst.working_directory.clone() }
}

/// Quotes one argument so `CommandLineToArgvW` (and MSVC's runtime) parse it
/// back unchanged.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// Splits a command line the way `CommandLineToArgvW` does for arguments
/// after the program name. Used for tests and for Steam's `%command%`.
pub fn split_command_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let start = i;
            while i < chars.len() && chars[i] == '\\' {
                i += 1;
            }
            let n = i - start;
            if i < chars.len() && chars[i] == '"' {
                cur.extend(std::iter::repeat_n('\\', n / 2));
                if n % 2 == 1 {
                    cur.push('"');
                    i += 1;
                }
            } else {
                cur.extend(std::iter::repeat_n('\\', n));
            }
            has_token = true;
            continue;
        }
        if c == '"' {
            if in_quotes && chars.get(i + 1) == Some(&'"') {
                cur.push('"');
                i += 2;
                continue;
            }
            in_quotes = !in_quotes;
            has_token = true;
        } else if (c == ' ' || c == '\t') && !in_quotes {
            if has_token {
                args.push(std::mem::take(&mut cur));
                has_token = false;
            }
        } else {
            cur.push(c);
            has_token = true;
        }
        i += 1;
    }
    if has_token {
        args.push(cur);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn inst() -> Installation {
        Installation::new(
            "Witcher 3 Ascended",
            PathBuf::from(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe"),
        )
    }

    #[test]
    fn open_mo2_uses_install_folder_and_no_args() {
        let spec = build(&inst(), LaunchMode::OpenMo2);
        assert_eq!(spec.program, Path::new(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe"));
        assert_eq!(spec.working_directory, Path::new(r"C:\Modding\Wabbajack Lists\Witcher 3 Ascended"));
        assert!(spec.args.is_empty());
        assert_eq!(spec.command_line(), r#""C:\Modding\Wabbajack Lists\Witcher 3 Ascended\ModOrganizer.exe""#);
    }

    #[test]
    fn play_uses_portable_shortcut_with_colon_title() {
        let mut i = inst();
        i.play_action = PlayAction::Executable { title: "The Witcher 3: Wild Hunt".into() };
        let spec = build(&i, LaunchMode::Play);
        assert_eq!(spec.args, vec!["moshortcut://:The Witcher 3: Wild Hunt"]);
        // Opening MO2 never runs the play executable.
        assert!(build(&i, LaunchMode::OpenMo2).args.is_empty());
    }

    #[test]
    fn play_without_executable_opens_mo2() {
        assert!(build(&inst(), LaunchMode::Play).args.is_empty());
    }

    #[test]
    fn profile_is_passed_first() {
        let mut i = inst();
        i.profile = Some("Custom - Ascended".into());
        i.play_action = PlayAction::Executable { title: "Game".into() };
        let spec = build(&i, LaunchMode::Play);
        assert_eq!(spec.args, vec!["-p", "Custom - Ascended", "moshortcut://:Game"]);
        assert_eq!(split_command_line(&spec.command_line())[1..], spec.args[..]);
    }

    #[test]
    fn quoting_round_trips() {
        let cases = [
            "plain",
            "with space",
            "",
            r"C:\Program Files (x86)\Steam\",
            r#"say "hi""#,
            r#"tricky \"quoted\" \\"#,
            "moshortcut://:The Witcher 3: Wild Hunt",
            "tab\there",
        ];
        let line = cases.iter().map(|c| quote_arg(c)).collect::<Vec<_>>().join(" ");
        assert_eq!(split_command_line(&line), cases);
    }

    #[test]
    fn split_handles_steam_command() {
        let line =
            r#""C:\Program Files (x86)\Steam\steamapps\common\Cyberpunk 2077\REDprelauncher.exe" -skipStartScreen"#;
        assert_eq!(
            split_command_line(line),
            vec![
                r"C:\Program Files (x86)\Steam\steamapps\common\Cyberpunk 2077\REDprelauncher.exe",
                "-skipStartScreen"
            ]
        );
    }
}
