use serde::{Deserialize, Serialize};

/// Display name of the application.
pub const APP_NAME: &str = "Settlement Generator";

/// Installed application version, derived from the workspace `Cargo.toml`
/// (the single canonical source, see `docs/VERSIONING.md`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application identity shared with both interfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    /// Raw semantic version without a `v` prefix, e.g. `0.1.0`.
    pub version: String,
}

impl AppInfo {
    /// Version formatted for display, e.g. `v0.1.0`.
    pub fn display_version(&self) -> String {
        format!("v{}", self.version)
    }
}

pub fn app_info() -> AppInfo {
    AppInfo {
        name: APP_NAME.to_owned(),
        version: VERSION.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_triplet() {
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "expected MAJOR.MINOR.PATCH, got {VERSION}");
        for part in parts {
            part.parse::<u64>()
                .unwrap_or_else(|_| panic!("non-numeric version component in {VERSION}"));
        }
    }

    #[test]
    fn app_info_matches_constants() {
        let info = app_info();
        assert_eq!(info.name, APP_NAME);
        assert_eq!(info.version, VERSION);
        assert_eq!(info.display_version(), format!("v{VERSION}"));
    }

    #[test]
    fn app_info_serialises_with_stable_field_names() {
        let json = serde_json::to_value(app_info()).unwrap();
        assert_eq!(json["name"], APP_NAME);
        assert_eq!(json["version"], VERSION);
    }
}
