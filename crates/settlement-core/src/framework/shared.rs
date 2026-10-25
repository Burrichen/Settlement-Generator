//! Shared results: filling the table entries that list one.

use crate::content::{ContentSet, TableResult};

use super::SharedResult;

/// Fills every field a table entry leaves empty from the shared result it
/// lists: name, description, tags, eligibility, value, label variants and
/// generator. Faces, `enabled`, uniqueness and fallback always belong to the
/// table. Entries listing an unknown shared result are left as they are
/// (validation reports them).
pub fn link_shared_results(content: &mut ContentSet) {
    let library = std::mem::take(&mut content.shared_results);
    for table in &mut content.tables {
        for result in &mut table.results {
            let Some(shared) = result
                .shared
                .as_ref()
                .and_then(|id| library.iter().find(|s| &s.id == id))
            else {
                continue;
            };
            fill(result, shared);
        }
    }
    content.shared_results = library;
}

fn fill(result: &mut TableResult, shared: &SharedResult) {
    if result.name.is_empty() {
        result.name = shared.name.clone();
    }
    if result.description.is_empty() {
        result.description = shared.description.clone();
    }
    if result.tags.is_empty() {
        result.tags = shared.tags.clone();
    }
    if result.eligibility.is_none() {
        result.eligibility = shared.eligibility.clone();
    }
    if result.value.is_none() {
        result.value = shared.value;
    }
    if result.label_variants.is_empty() {
        result.label_variants = shared.label_variants.clone();
    }
    if result.generator.is_none() {
        result.generator = shared.generator.clone();
    }
}

/// After the shared results changed from `before` to the content's own
/// (e.g. a user customised one), carries each change into the table entries
/// that still use the old value, so a table that kept a field of its own
/// keeps it.
pub fn relink_shared_results(content: &mut ContentSet, before: &[SharedResult]) {
    let library = std::mem::take(&mut content.shared_results);
    for table in &mut content.tables {
        for result in &mut table.results {
            let Some(id) = &result.shared else { continue };
            let (Some(old), Some(new)) = (
                before.iter().find(|s| &s.id == id),
                library.iter().find(|s| &s.id == id),
            ) else {
                continue;
            };
            if old == new {
                continue;
            }
            if result.name == old.name {
                result.name = new.name.clone();
            }
            if result.description == old.description {
                result.description = new.description.clone();
            }
            if result.tags == old.tags {
                result.tags = new.tags.clone();
            }
            if result.eligibility == old.eligibility {
                result.eligibility = new.eligibility.clone();
            }
            if result.value == old.value {
                result.value = new.value;
            }
            if result.label_variants == old.label_variants {
                result.label_variants = new.label_variants.clone();
            }
            if result.generator == old.generator {
                result.generator = new.generator.clone();
            }
        }
    }
    content.shared_results = library;
}
