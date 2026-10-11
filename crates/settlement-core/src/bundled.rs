//! Bundled default content: every `content/*.json` file, compiled into every
//! binary so the GUI and the terminal interface ship identical defaults for a
//! given version. Each file is a partial [`ContentSet`]; they are merged in
//! file-name order.

use thiserror::Error;

use crate::content::ContentSet;

include!(concat!(env!("OUT_DIR"), "/bundled_files.rs"));

#[derive(Debug, Error)]
pub enum BundledContentError {
    #[error("bundled content file `{file}` is not valid: {source}")]
    Parse {
        file: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("bundled content files `{first}` and `{second}` both define the generation sequence")]
    DuplicateSequence {
        first: &'static str,
        second: &'static str,
    },
    #[error("bundled content file `{file}` uses schema {found}, but others use {expected}")]
    SchemaMismatch {
        file: &'static str,
        found: u32,
        expected: u32,
    },
}

/// Names of the bundled content files, in merge order.
pub fn bundled_file_names() -> impl Iterator<Item = &'static str> {
    BUNDLED_FILES.iter().map(|(name, _)| *name)
}

/// The default content shipped with this application version.
pub fn bundled_content() -> Result<ContentSet, BundledContentError> {
    let mut merged = ContentSet::default();
    let mut sequence_file: Option<&'static str> = None;
    for (index, (file, json)) in BUNDLED_FILES.iter().enumerate() {
        let part = ContentSet::from_json(json)
            .map_err(|source| BundledContentError::Parse { file, source })?;
        if index == 0 {
            merged.schema_version = part.schema_version;
        } else if part.schema_version != merged.schema_version {
            return Err(BundledContentError::SchemaMismatch {
                file,
                found: part.schema_version,
                expected: merged.schema_version,
            });
        }
        if !part.sequence.is_empty() {
            if let Some(first) = sequence_file {
                return Err(BundledContentError::DuplicateSequence {
                    first,
                    second: file,
                });
            }
            sequence_file = Some(file);
            merged.sequence = part.sequence;
        }
        merged.tables.extend(part.tables);
        merged.tags.extend(part.tags);
        merged.naming_schemes.extend(part.naming_schemes);
    }
    Ok(merged)
}
