//! Built-in steps: stages the application provides because they have their
//! own editing screens. A flow names one by `handler` and configures it with
//! data (`config`), so which tables, tags and fields it uses is content, not
//! code. Built-in steps run only at the top level of the start flow.

use std::fmt;

use crate::content::Condition;
use crate::figures::FiguresConfig;
use crate::ids::{TableId, TagId};
use crate::locations::LocationsConfig;
use crate::roles::RolesConfig;

use super::StepAction;

/// Allocates a Role / Title to each representative of the Authority.
pub const ROLES_HANDLER: &str = "authority_roles";
/// Generates the Authority Figures and their relationships.
pub const FIGURES_HANDLER: &str = "authority_figures";
/// Generates the Notable Locations.
pub const LOCATIONS_HANDLER: &str = "notable_locations";
/// Every built-in step this build provides.
pub const HANDLERS: [&str; 3] = [ROLES_HANDLER, FIGURES_HANDLER, LOCATIONS_HANDLER];

/// A built-in step with its configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum Builtin {
    Roles(RolesConfig),
    Figures(FiguresConfig),
    Locations(LocationsConfig),
}

/// Why a built-in step cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinError {
    Unknown(String),
    Config { handler: String, error: String },
}

impl fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuiltinError::Unknown(handler) => write!(
                f,
                "there is no built-in step `{handler}` (available: {})",
                HANDLERS.join(", ")
            ),
            BuiltinError::Config { handler, error } => {
                write!(
                    f,
                    "built-in step `{handler}` has an invalid config: {error}"
                )
            }
        }
    }
}

impl Builtin {
    /// Reads `config` for `handler`.
    pub fn parse(handler: &str, config: &serde_json::Value) -> Result<Self, BuiltinError> {
        fn read<T: serde::de::DeserializeOwned>(
            handler: &str,
            config: &serde_json::Value,
        ) -> Result<T, BuiltinError> {
            serde_json::from_value(config.clone()).map_err(|e| BuiltinError::Config {
                handler: handler.to_owned(),
                error: e.to_string(),
            })
        }
        match handler {
            ROLES_HANDLER => read(handler, config).map(Builtin::Roles),
            FIGURES_HANDLER => read(handler, config).map(Builtin::Figures),
            LOCATIONS_HANDLER => read(handler, config).map(Builtin::Locations),
            other => Err(BuiltinError::Unknown(other.to_owned())),
        }
    }

    pub fn handler(&self) -> &'static str {
        match self {
            Builtin::Roles(_) => ROLES_HANDLER,
            Builtin::Figures(_) => FIGURES_HANDLER,
            Builtin::Locations(_) => LOCATIONS_HANDLER,
        }
    }

    /// Every table the configuration names.
    pub fn tables(&self) -> Vec<&TableId> {
        match self {
            Builtin::Roles(c) => c.authority.tables().to_vec(),
            Builtin::Figures(c) => c.authority.tables().into_iter().chain(c.tables()).collect(),
            Builtin::Locations(c) => vec![&c.size_table, &c.type_table],
        }
    }

    /// Every tag the configuration names.
    pub fn tags(&self) -> Vec<&TagId> {
        match self {
            Builtin::Roles(c) => c.authority.tags().to_vec(),
            Builtin::Figures(c) => c.authority.tags().to_vec(),
            Builtin::Locations(_) => Vec::new(),
        }
    }

    /// Every condition the configuration holds.
    pub fn conditions(&self) -> Vec<&Condition> {
        match self {
            Builtin::Locations(c) => c.guaranteed.iter().map(|g| &g.when).collect(),
            Builtin::Roles(_) | Builtin::Figures(_) => Vec::new(),
        }
    }
}

/// The step a legacy `sequence` marker stands for: its ID and action, with
/// the configuration the legacy format implied.
pub(crate) fn legacy_step(handler: &str) -> (String, StepAction) {
    let config = match handler {
        ROLES_HANDLER => serde_json::to_value(RolesConfig::legacy()),
        FIGURES_HANDLER => serde_json::to_value(FiguresConfig::legacy()),
        _ => serde_json::to_value(LocationsConfig::legacy()),
    }
    .expect("configuration serialises");
    (
        handler.to_owned(),
        StepAction::Builtin {
            handler: handler.to_owned(),
            config,
        },
    )
}
