//! Finds portable MO2 installations under user-chosen folders. Discovery only
//! reports candidates; adding them is always a separate, confirmed step.

use std::path::{Path, PathBuf};

use crate::mo2::{self, MO2_EXE, Mo2Install};

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub max_depth: usize,
    pub max_directories: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { max_depth: 5, max_directories: 20_000 }
    }
}

#[derive(Debug)]
pub struct Candidate {
    pub exe: PathBuf,
    pub suggested_name: String,
    pub install: Result<Mo2Install, String>,
    pub already_added: bool,
}

impl Candidate {
    /// Portable installs that are not already in the list are pre-selected.
    pub fn recommended(&self) -> bool {
        !self.already_added && self.install.as_ref().is_ok_and(|i| i.portable)
    }
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<(PathBuf, String)>,
    pub truncated: bool,
}

impl ScanReport {
    /// Candidates worth announcing at startup: the recommended ones (portable,
    /// readable, not yet added) that the user has not dismissed before.
    pub fn new_modlists(&self, dismissed: &[PathBuf]) -> Vec<&Candidate> {
        self.candidates
            .iter()
            .filter(|c| c.recommended() && !dismissed.iter().any(|d| crate::paths::same_path(d, &c.exe)))
            .collect()
    }
}

pub fn scan(roots: &[PathBuf], options: ScanOptions, already_added: impl Fn(&Path) -> bool) -> ScanReport {
    let mut report = ScanReport::default();
    let mut visited = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().rev().map(|r| (r.clone(), 0)).collect();
    let mut seen = std::collections::HashSet::new();

    while let Some((dir, depth)) = stack.pop() {
        if !seen.insert(crate::paths::normalize(&dir)) {
            continue;
        }
        visited += 1;
        if visited > options.max_directories {
            report.truncated = true;
            break;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                report.skipped.push((dir, e.to_string()));
                continue;
            }
        };
        let mut subdirs = Vec::new();
        let mut exe = None;
        for entry in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.is_file() && entry.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(MO2_EXE)) {
                exe = Some(entry.path());
            } else if meta.is_dir() && !is_link(&meta) && !skip_dir(&entry.file_name()) {
                subdirs.push(entry.path());
            }
        }
        if let Some(exe) = exe {
            // An MO2 folder holds mods and downloads; never descend into it.
            let install = mo2::read_install(&exe).map_err(|e| e.to_string());
            report.candidates.push(Candidate {
                suggested_name: suggest_name(&exe, install.as_ref().ok()),
                already_added: already_added(&exe),
                install,
                exe,
            });
            continue;
        }
        if depth < options.max_depth {
            subdirs.sort();
            stack.extend(subdirs.into_iter().rev().map(|d| (d, depth + 1)));
        }
    }
    report.candidates.sort_by(|a, b| a.exe.cmp(&b.exe));
    report
}

fn skip_dir(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else { return true };
    name.starts_with('$')
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("node_modules")
        || name.eq_ignore_ascii_case(".git")
}

#[cfg(windows)]
fn is_link(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    meta.file_type().is_symlink() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

pub fn suggest_name(exe: &Path, install: Option<&Mo2Install>) -> String {
    if let Some(name) = install.and_then(|i| i.modlist_name.clone()) {
        return name;
    }
    exe.parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .or_else(|| install.and_then(|i| i.game_name.clone()))
        .unwrap_or_else(|| "Mod Organizer 2".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mo2::fixtures::portable;

    #[test]
    fn finds_portable_installs_and_stops_inside_them() {
        let tmp = tempfile::tempdir().unwrap();
        let a = portable(
            &tmp.path().join("Modlists"),
            "ProjectUltrapunk",
            "[General]\ngameName=Cyberpunk 2077\n",
            &["Default"],
        );
        std::fs::write(a.root.join("Project Ultrapunk.compiler_settings"), "{}").unwrap();
        let b = portable(
            &tmp.path().join("Modding/Wabbajack Lists"),
            "Witcher 3 Ascended",
            "[General]\ngameName=The Witcher 3\n",
            &[],
        );
        // A nested copy inside a mods folder must not be reported.
        portable(&a.root.join("mods/some mod"), "inner", "", &[]);
        // A non-portable install is reported but not recommended.
        let global = tmp.path().join("Other/MO2");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(global.join("ModOrganizer.exe"), b"").unwrap();

        let report = scan(&[tmp.path().to_path_buf()], ScanOptions::default(), |p| crate::paths::same_path(p, &b.exe));
        let found: Vec<_> = report.candidates.iter().map(|c| c.exe.clone()).collect();
        assert_eq!(found.len(), 3, "{found:?}");
        let ultra = report.candidates.iter().find(|c| c.exe == a.exe).unwrap();
        assert_eq!(ultra.suggested_name, "Project Ultrapunk");
        assert!(ultra.recommended());
        let witcher = report.candidates.iter().find(|c| c.exe == b.exe).unwrap();
        assert_eq!(witcher.suggested_name, "Witcher 3 Ascended");
        assert!(witcher.already_added);
        assert!(!witcher.recommended());
        let other = report.candidates.iter().find(|c| c.exe == global.join("ModOrganizer.exe")).unwrap();
        assert!(!other.recommended());
        assert!(!report.truncated);
    }

    #[test]
    fn new_modlists_skip_added_dismissed_and_non_portable() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Modlists");
        let fresh = portable(
            &root,
            "Fresh List",
            "[General]
gameName=Skyrim Special Edition
",
            &[],
        );
        let added = portable(
            &root,
            "Added List",
            "[General]
gameName=Fallout 4
",
            &[],
        );
        let dismissed = portable(
            &root,
            "Dismissed List",
            "[General]
gameName=Starfield
",
            &[],
        );
        let global = root.join("Global MO2");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::write(global.join("ModOrganizer.exe"), b"").unwrap();

        let report =
            scan(std::slice::from_ref(&root), ScanOptions::default(), |p| crate::paths::same_path(p, &added.exe));
        assert_eq!(report.candidates.len(), 4);
        // Dismissed paths match regardless of case, as Windows paths do.
        let dismissed_paths = vec![PathBuf::from(dismissed.exe.to_string_lossy().to_uppercase())];
        let found: Vec<_> = report.new_modlists(&dismissed_paths).iter().map(|c| c.exe.clone()).collect();
        assert_eq!(found, vec![fresh.exe.clone()]);
        assert_eq!(report.new_modlists(&[]).len(), 2);
    }

    #[test]
    fn respects_depth_limit() {
        let tmp = tempfile::tempdir().unwrap();
        portable(&tmp.path().join("a/b/c/d"), "deep", "", &[]);
        let shallow = scan(&[tmp.path().to_path_buf()], ScanOptions { max_depth: 2, ..Default::default() }, |_| false);
        assert!(shallow.candidates.is_empty());
        let deep = scan(&[tmp.path().to_path_buf()], ScanOptions { max_depth: 5, ..Default::default() }, |_| false);
        assert_eq!(deep.candidates.len(), 1);
    }

    #[test]
    fn caps_directory_count() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..20 {
            std::fs::create_dir_all(tmp.path().join(format!("d{i}"))).unwrap();
        }
        let report = scan(&[tmp.path().to_path_buf()], ScanOptions { max_depth: 3, max_directories: 5 }, |_| false);
        assert!(report.truncated);
    }

    #[test]
    fn missing_root_is_reported_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let report = scan(&[tmp.path().join("does-not-exist")], ScanOptions::default(), |_| false);
        assert!(report.candidates.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn scanning_does_not_modify_installs() {
        let tmp = tempfile::tempdir().unwrap();
        let a = portable(tmp.path(), "L", "[General]\ngameName=X\n", &["Default"]);
        let before = crate::mo2::fixtures::snapshot(&a.root);
        scan(&[tmp.path().to_path_buf()], ScanOptions::default(), |_| false);
        assert_eq!(before, crate::mo2::fixtures::snapshot(&a.root));
    }

    #[cfg(windows)]
    #[test]
    fn does_not_follow_junctions() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real");
        portable(&target, "L", "", &[]);
        let scan_root = tmp.path().join("scan");
        std::fs::create_dir_all(&scan_root).unwrap();
        let link = scan_root.join("link");
        let status =
            std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&target).output().unwrap();
        if !status.status.success() {
            return; // junction creation unavailable; nothing to test
        }
        let report = scan(&[scan_root], ScanOptions::default(), |_| false);
        assert!(report.candidates.is_empty());
    }
}
