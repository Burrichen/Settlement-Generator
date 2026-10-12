//! Stable internal identifiers.
//!
//! IDs are distinct from editable display names: content always references
//! other content by ID, so renaming never breaks a reference. See
//! `docs/DATA_MODEL.md` §1.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Prefix reserved for IDs of user-created content. Built-in content never uses
/// it, so bundled updates cannot collide with user additions.
pub const USER_ID_PREFIX: &str = "user.";

/// Returns `true` if `id` matches `^[A-Za-z0-9][A-Za-z0-9_.-]*$` (ASCII,
/// case-sensitive, e.g. `world.lowFantasy`).
pub fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_valid(&self) -> bool {
                is_valid_id(&self.0)
            }

            /// `true` for IDs in the reserved user namespace.
            pub fn is_user(&self) -> bool {
                self.0.starts_with(USER_ID_PREFIX)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self::new(id)
            }
        }
    };
}

define_id!(
    /// Identifies a table. Unique across all tables.
    TableId
);
define_id!(
    /// Identifies a tag. Unique across all tags.
    TagId
);
define_id!(
    /// Identifies a result. Unique within its table.
    ResultId
);
define_id!(
    /// Identifies a table variant. Unique within its table.
    VariantId
);
define_id!(
    /// Identifies a naming scheme. Unique across all naming schemes.
    NamingSchemeId
);
define_id!(
    /// Identifies a generated person (NPC), e.g. an Authority Figure.
    /// Stable for the life of the person; relationships refer to it.
    NpcId
);

impl NpcId {
    /// A fresh random ID, `npc-` and 32 hex digits.
    pub fn new_random() -> Self {
        let bits: u128 = rand::random();
        NpcId::new(format!("npc-{bits:032x}"))
    }
}

define_id!(
    /// Identifies a role scheme: the role tables an Authority uses.
    RoleSchemeId
);
define_id!(
    /// Identifies a location scheme: the Notable Location tables a Size uses.
    LocationSchemeId
);
define_id!(
    /// Identifies a subordinate generator that elaborates a result, such as
    /// the NPCs who hold an Authority. Reserved: no generators exist yet.
    GeneratorId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_ids() {
        for id in [
            "a",
            "table.economy",
            "user.0f3a-9",
            "tag_x",
            "9lives",
            "world.lowFantasy",
        ] {
            assert!(is_valid_id(id), "{id} should be valid");
        }
    }

    #[test]
    fn invalid_ids() {
        for id in ["", ".x", "-x", "a b", "a/b", "é", "_x"] {
            assert!(!is_valid_id(id), "{id:?} should be invalid");
        }
    }

    #[test]
    fn user_namespace() {
        assert!(TagId::from("user.abc").is_user());
        assert!(!TagId::from("tag.user").is_user());
    }

    #[test]
    fn serialises_transparently() {
        let id = TableId::from("table.economy");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"table.economy\"");
        let back: TableId = serde_json::from_str("\"table.economy\"").unwrap();
        assert_eq!(back, id);
    }
}
