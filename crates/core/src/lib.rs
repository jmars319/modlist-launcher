//! Platform-neutral logic for Modlist Launcher.
//!
//! Nothing in this crate writes inside a Mod Organizer 2 installation. MO2 files
//! are only ever opened for reading; every launcher-owned write goes through
//! [`config::DataDir`], which refuses paths outside the launcher's data folder.

pub mod cli;
pub mod config;
pub mod discovery;
pub mod ini;
pub mod launch;
pub mod mo2;
pub mod paths;
pub mod process;
pub mod steam;
pub mod validate;
