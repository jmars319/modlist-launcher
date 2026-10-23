//! Steam integration: library lookup, local artwork, and the per-game launch
//! options that let Steam's Play button start a modlist.

pub mod launch_options;
pub mod library;
pub mod vdf;

pub use launch_options::{RestorePlan, get_launch_options, plan_restore, set_launch_options, wrapper_launch_options};
pub use library::{Account, AppInfo, SteamRoot};
