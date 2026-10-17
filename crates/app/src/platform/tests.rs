//! Integration tests against real Windows processes. The "MO2" here is a copy
//! of this test binary renamed to ModOrganizer.exe; nothing real is started.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use modlist_core::config::Installation;
use modlist_core::launch::{self, LaunchMode};
use modlist_core::process::{self, Gate};

use super::{launch as plaunch, processes};

const CWD_FILE_ENV: &str = "MODLIST_TEST_CWD_FILE";

/// Helper run inside the fake ModOrganizer.exe: records its working directory,
/// then stays alive briefly so it can be detected.
#[test]
#[ignore = "helper for fake_mo2_is_detected_and_starts_in_its_folder"]
fn helper_fake_mo2() {
    if let Some(file) = std::env::var_os(CWD_FILE_ENV) {
        std::fs::write(file, std::env::current_dir().unwrap().to_string_lossy().as_bytes()).unwrap();
        std::thread::sleep(Duration::from_secs(8));
    }
}

#[test]
fn fake_mo2_is_detected_and_starts_in_its_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Fake List");
    std::fs::create_dir_all(&root).unwrap();
    let exe = root.join("ModOrganizer.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    let cwd_file = tmp.path().join("cwd.txt");

    let inst = Installation::new("Fake List", exe.clone());
    let mut spec = launch::build(&inst, LaunchMode::OpenMo2);
    spec.args = vec!["--exact".into(), "platform::tests::helper_fake_mo2".into(), "--ignored".into()];
    // SAFETY: tests in this binary that read this variable only run in the child.
    unsafe { std::env::set_var(CWD_FILE_ENV, &cwd_file) };
    let mut child = plaunch::spawn(&spec).unwrap();
    unsafe { std::env::remove_var(CWD_FILE_ENV) };

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut found = None;
    while Instant::now() < deadline && found.is_none() {
        let running = processes::running(|n| n.eq_ignore_ascii_case("ModOrganizer.exe"));
        found = process::is_running(&exe, &running);
        if found.is_none() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let pid = found.expect("fake MO2 was not detected by full path");
    // Only the fake instance: a real MO2 may be open on the machine running the tests.
    let running: Vec<_> = processes::running(|n| n.eq_ignore_ascii_case("ModOrganizer.exe"))
        .into_iter()
        .filter(|p| p.pid == pid)
        .collect();
    assert_eq!(process::gate(&exe, &running, std::slice::from_ref(&inst)), Gate::SameInstanceRunning { pid });
    let other = Installation::new("Other", PathBuf::from(r"C:\Elsewhere\ModOrganizer.exe"));
    assert!(matches!(
        process::gate(&other.exe_path, &running, &[inst.clone(), other.clone()]),
        Gate::OtherInstanceRunning { name: Some(ref n), .. } if n == "Fake List"
    ));

    while Instant::now() < deadline && !cwd_file.exists() {
        std::thread::sleep(Duration::from_millis(100));
    }
    let cwd = std::fs::read_to_string(&cwd_file).expect("child did not record its working directory");
    assert!(modlist_core::paths::same_path(std::path::Path::new(&cwd), &root), "cwd was {cwd}");

    let _ = child.kill();
    let _ = child.wait();
    assert!(processes::wait_for_exit(pid, Some(Duration::from_secs(5))));
}

#[test]
fn spawn_refuses_missing_working_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let inst = Installation::new("Gone", tmp.path().join("gone/ModOrganizer.exe"));
    let spec = launch::build(&inst, LaunchMode::OpenMo2);
    assert!(plaunch::spawn(&spec).is_err());
}

#[test]
fn shortcut_names_are_sanitized() {
    assert_eq!(super::shell::sanitize_file_name("The Witcher 3: Wild Hunt?"), "The Witcher 3  Wild Hunt");
    assert_eq!(super::shell::sanitize_file_name("..."), "Modlist");
}
