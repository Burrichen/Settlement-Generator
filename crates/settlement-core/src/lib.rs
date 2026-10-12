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
pub mod dice;
pub mod editor;
pub mod engine;
pub mod figures;
pub mod ids;
pub mod layers;
mod layout;
pub mod naming;
pub mod roadmap;
pub mod service;
pub mod session;
pub mod settings;
pub mod settlements;
pub mod storage;
pub mod validation;

pub use app_info::{APP_NAME, AppInfo, VERSION, app_info};
