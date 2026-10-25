//! Reading stored map documents across format versions.
//!
//! Every stored map document (map set header, each map, the metadata) is
//! saved with [`MAP_FORMAT_VERSION`]. To read one:
//!
//! 1. a newer format is refused (this build cannot know what it means);
//! 2. an older format is parsed as JSON and upgraded one version at a time by
//!    the steps in [`UPGRADES`], then deserialised.
//!
//! Upgrading happens in memory only; the stored row is rewritten in the new
//! format the next time it is saved. A format change therefore means: bump
//! [`MAP_FORMAT_VERSION`], append a step to [`UPGRADES`], and add a test
//! that reads a document of the old format. Released steps are never
//! edited.
//!
//! Geometry is never re-derived to upgrade a map: a step may rename or
//! restructure fields, but must keep every coordinate and ID as saved.

use serde::de::{DeserializeOwned, Error as _};
use serde_json::Value;

use super::model::MAP_FORMAT_VERSION;

/// One upgrade step: a document of format `n` (index `n - 1`) to `n + 1`.
type Upgrade = fn(Value) -> Result<Value, String>;

/// The upgrade steps, oldest first. Format 1 is the first, so none yet.
const UPGRADES: &[Upgrade] = &[];

#[derive(Debug)]
pub enum ReadError {
    Newer { found: u32 },
    Corrupt(serde_json::Error),
}

/// Reads a stored document saved in `format`.
pub fn read_document<T: DeserializeOwned>(format: u32, json: &str) -> Result<T, ReadError> {
    read_with(format, json, MAP_FORMAT_VERSION, UPGRADES)
}

fn read_with<T: DeserializeOwned>(
    format: u32,
    json: &str,
    current: u32,
    upgrades: &[Upgrade],
) -> Result<T, ReadError> {
    if format > current {
        return Err(ReadError::Newer { found: format });
    }
    if format == 0 {
        return Err(ReadError::Corrupt(serde_json::Error::custom(
            "map format 0 does not exist",
        )));
    }
    let mut value: Value = serde_json::from_str(json).map_err(ReadError::Corrupt)?;
    for step in &upgrades[(format - 1) as usize..(current - 1) as usize] {
        value = step(value).map_err(|e| ReadError::Corrupt(serde_json::Error::custom(e)))?;
    }
    serde_json::from_value(value).map_err(ReadError::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct V2 {
        label: String,
    }

    fn rename_name_to_label(mut v: Value) -> Result<Value, String> {
        let obj = v.as_object_mut().ok_or("not an object")?;
        let name = obj.remove("name").ok_or("no name")?;
        obj.insert("label".into(), name);
        Ok(v)
    }

    #[test]
    fn there_is_one_step_per_format_after_the_first() {
        assert_eq!(UPGRADES.len() as u32, MAP_FORMAT_VERSION - 1);
    }

    #[test]
    fn older_documents_are_upgraded_step_by_step() {
        let steps: &[Upgrade] = &[rename_name_to_label];
        let v: V2 = read_with(1, r#"{"name":"Harbour"}"#, 2, steps).unwrap();
        assert_eq!(v.label, "Harbour");
        let current: V2 = read_with(2, r#"{"label":"Harbour"}"#, 2, steps).unwrap();
        assert_eq!(current, v);
    }

    #[test]
    fn newer_and_corrupt_documents_are_refused() {
        assert!(matches!(
            read_document::<Value>(MAP_FORMAT_VERSION + 1, "{}"),
            Err(ReadError::Newer { .. })
        ));
        assert!(matches!(
            read_document::<Value>(1, "not json"),
            Err(ReadError::Corrupt(_))
        ));
        assert!(matches!(
            read_document::<Value>(0, "{}"),
            Err(ReadError::Corrupt(_))
        ));
    }
}
