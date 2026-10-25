//! Shared core for Settlement Generator.
//!
//! All business logic lives here. The GUI (`settlement-gui`) and the terminal
//! interface (`settlement-cli`) are thin adapters over this crate.

mod app_info;
pub mod authority;
pub mod backup;
pub mod bundled;
pub mod content;
pub mod db;
pub mod dependencies;
pub mod dice;
pub mod dice_assist;
pub mod editor;
pub mod engine;
pub mod explain;
pub mod figures;
pub mod framework;
pub mod ids;
pub mod layers;
mod layout;
pub mod locations;
pub mod naming;
pub mod roadmap;
pub mod roles;
pub mod service;
pub mod session;
pub mod settings;
pub mod settlements;
pub mod simulate;
pub mod storage;
pub mod table_editor;
pub mod trace;
pub mod transfer;
pub mod validation;

pub use app_info::{APP_NAME, AppInfo, VERSION, app_info};
