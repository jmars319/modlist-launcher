#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod cards;
mod gui;
mod headless;
mod launching;
mod platform;
mod state;

slint::include_modules!();

use modlist_core::cli::{self, Command};

fn main() {
    let command = match cli::parse(std::env::args_os()) {
        Ok(c) => c,
        Err(e) => {
            headless::show_message("Modlist Launcher", &format!("{e}\n\n{}", cli::USAGE));
            std::process::exit(2);
        }
    };
    let code = match command {
        Command::Gui => gui::run(),
        Command::Launch { target, mode } => headless::launch(&target, mode),
        Command::Steam { target, original } => headless::steam(&target, &original),
        Command::Help => {
            headless::show_message("Modlist Launcher", cli::USAGE);
            0
        }
        Command::Version => {
            headless::show_message("Modlist Launcher", &format!("Version {}", env!("CARGO_PKG_VERSION")));
            0
        }
    };
    std::process::exit(code);
}
