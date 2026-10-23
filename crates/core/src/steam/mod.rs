//! Steam integration: library lookup, local artwork, and the per-game launch
//! options that let Steam's Play button start a modlist.

pub mod guess;
pub mod launch_options;
pub mod library;
pub mod vdf;

pub use guess::guess_app_id;
pub use launch_options::{
    RestorePlan, get_launch_options, launcher_connections, launcher_target, original_launch_options, plan_restore,
    set_launch_options, wrapper_launch_options,
};
pub use library::{Account, AppInfo, SteamRoot};
