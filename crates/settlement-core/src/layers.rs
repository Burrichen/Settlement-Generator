//! Content layering: bundled defaults + user overrides = effective content.
//!
//! Bundled defaults ship with the application and are never modified at
//! runtime. Every user customisation lives in [`UserOverrides`], keyed by
//! stable ID, and always takes precedence. Overrides are stored as field-level
//! patches, so an application update that changes a default only affects the
//! fields the user has not customised. See `docs/DATA_MODEL.md` §4.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::content::{
    Condition, ContentSet, EffectDef, FaceRange, FollowUpRoll, Table, TableResult, TableVariant,
    Tag,
};
use crate::engine::{GenerationError, Generator};
use crate::ids::{ResultId, TableId, TagId, USER_ID_PREFIX};
use crate::validation::{
    IdScope, IssueKind, Location, Severity, ValidationIssue, ValidationReport, validate,
};

/// Version of the user override file format understood by this build.
pub const OVERRIDES_FORMAT_VERSION: u32 = 1;

fn default_format_version() -> u32 {
    OVERRIDES_FORMAT_VERSION
}

/// Fingerprint of a bundled item at the time it was customised, used to tell
/// the user when a default changed underneath their customisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(String);

impl Fingerprint {
    fn of<T: Serialize>(value: &T) -> Self {
        // FNV-1a over canonical JSON: stable across runs and platforms.
        let bytes = serde_json::to_vec(value).expect("content serialises");
        let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        });
        Fingerprint(format!("{hash:016x}"))
    }

    /// A table's own fields, excluding results (patched individually) and
    /// variants (not customisable).
    fn of_table_header(table: &Table) -> Self {
        let mut header = table.clone();
        header.results.clear();
        header.variants.clear();
        Self::of(&header)
    }
}

/// All user customisations of generator content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserOverrides {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<TableId, TableOverride>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<TagId, TagOverride>,
}

impl Default for UserOverrides {
    fn default() -> Self {
        Self {
            format_version: OVERRIDES_FORMAT_VERSION,
            tables: BTreeMap::new(),
            tags: BTreeMap::new(),
        }
    }
}

impl UserOverrides {
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty() && self.tags.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TableOverride {
    /// A table created by the user.
    Custom(Table),
    /// Changes to a bundled table.
    Modify(TablePatch),
    /// A bundled table the user deleted. The bundled resource is untouched.
    Delete,
}

/// Field-level changes to a bundled table. `None` means "use the default".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TablePatch {
    pub base: Fingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub dice: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub eligibility: Option<Option<Condition>>,
    /// Changes to the table's bundled results.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub results: BTreeMap<ResultId, ResultOverride>,
    /// User-created results, appended after the bundled ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_results: Vec<TableResult>,
    /// Replaces the whole list of tag-driven variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<TableVariant>>,
    /// Replaces the whole list of follow-up rolls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_ups: Option<Vec<FollowUpRoll>>,
}

impl TablePatch {
    fn new(base: &Table) -> Self {
        TablePatch {
            base: Fingerprint::of_table_header(base),
            name: None,
            description: None,
            dice: None,
            active: None,
            eligibility: None,
            results: BTreeMap::new(),
            added_results: Vec::new(),
            variants: None,
            follow_ups: None,
        }
    }

    /// Drops fields that equal the default.
    fn prune(&mut self, base: &Table) {
        prune_field(&mut self.name, &base.name);
        prune_field(&mut self.description, &base.description);
        prune_field(&mut self.dice, &base.dice);
        prune_field(&mut self.active, &base.active);
        prune_field(&mut self.eligibility, &base.eligibility);
        prune_field(&mut self.variants, &base.variants);
        prune_field(&mut self.follow_ups, &base.follow_ups);
    }

    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.dice.is_none()
            && self.active.is_none()
            && self.eligibility.is_none()
            && self.results.is_empty()
            && self.added_results.is_empty()
            && self.variants.is_none()
            && self.follow_ups.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ResultOverride {
    Modify(ResultPatch),
    /// A bundled result the user deleted.
    Delete,
}

/// Field-level changes to a bundled result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultPatch {
    pub base: Fingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub range: Option<Option<FaceRange>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<TagId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub eligibility: Option<Option<Condition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub value: Option<Option<i64>>,
}

impl ResultPatch {
    fn new(base: &TableResult) -> Self {
        ResultPatch {
            base: Fingerprint::of(base),
            name: None,
            description: None,
            range: None,
            tags: None,
            eligibility: None,
            enabled: None,
            value: None,
        }
    }

    fn prune(&mut self, base: &TableResult) {
        prune_field(&mut self.name, &base.name);
        prune_field(&mut self.description, &base.description);
        prune_field(&mut self.range, &base.range);
        prune_field(&mut self.tags, &base.tags);
        prune_field(&mut self.eligibility, &base.eligibility);
        prune_field(&mut self.enabled, &base.enabled);
        prune_field(&mut self.value, &base.value);
    }

    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.range.is_none()
            && self.tags.is_none()
            && self.eligibility.is_none()
            && self.enabled.is_none()
            && self.value.is_none()
    }

    fn apply(&self, result: &mut TableResult) {
        apply_field(&self.name, &mut result.name);
        apply_field(&self.description, &mut result.description);
        apply_field(&self.range, &mut result.range);
        apply_field(&self.tags, &mut result.tags);
        apply_field(&self.eligibility, &mut result.eligibility);
        apply_field(&self.enabled, &mut result.enabled);
        apply_field(&self.value, &mut result.value);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TagOverride {
    Custom(Tag),
    Modify(TagPatch),
    Delete,
}

/// Field-level changes to a bundled tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagPatch {
    pub base: Fingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Replaces the whole effect list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<Vec<EffectDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl TagPatch {
    fn new(base: &Tag) -> Self {
        TagPatch {
            base: Fingerprint::of(base),
            name: None,
            description: None,
            effects: None,
            enabled: None,
        }
    }

    fn prune(&mut self, base: &Tag) {
        prune_field(&mut self.name, &base.name);
        prune_field(&mut self.description, &base.description);
        prune_field(&mut self.effects, &base.effects);
        prune_field(&mut self.enabled, &base.enabled);
    }

    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.effects.is_none()
            && self.enabled.is_none()
    }
}

fn prune_field<T: PartialEq>(patch: &mut Option<T>, base: &T) {
    if patch.as_ref() == Some(base) {
        *patch = None;
    }
}

fn apply_field<T: Clone>(patch: &Option<T>, target: &mut T) {
    if let Some(value) = patch {
        *target = value.clone();
    }
}

/// Where an item in the effective content came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Unchanged bundled default.
    Bundled,
    /// Bundled default with user changes.
    Modified,
    /// Created by the user.
    Custom,
}

/// Provenance of everything in the effective content, plus deleted defaults
/// (so interfaces can offer to restore them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origins {
    pub tables: BTreeMap<TableId, Origin>,
    pub results: BTreeMap<TableId, BTreeMap<ResultId, Origin>>,
    pub tags: BTreeMap<TagId, Origin>,
    pub deleted_tables: BTreeSet<TableId>,
    pub deleted_results: BTreeMap<TableId, BTreeSet<ResultId>>,
    pub deleted_tags: BTreeSet<TagId>,
}

/// The effective content and everything known about how it was produced.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedContent {
    pub content: ContentSet,
    pub origins: Origins,
    /// Layering issues followed by validation of the merged content.
    pub report: ValidationReport,
}

impl MergedContent {
    /// A generator for the effective content, or a clear validation error if
    /// the customised content is not usable.
    pub fn generator(&self) -> Result<Generator<'_>, GenerationError> {
        if self.report.has_errors() {
            return Err(GenerationError::InvalidContent(self.report.clone()));
        }
        Generator::new(&self.content)
    }
}

/// Merges `overrides` over `bundled`. Never fails: problems are reported in
/// [`MergedContent::report`].
pub fn merge(bundled: &ContentSet, overrides: &UserOverrides) -> MergedContent {
    let mut issues = Vec::new();
    let mut origins = Origins::default();
    let mut content = ContentSet {
        schema_version: bundled.schema_version,
        sequence: bundled.sequence.clone(),
        tables: Vec::new(),
        tags: Vec::new(),
        // Naming schemes are content-file data; user overrides apply to
        // their method and component tables.
        naming_schemes: bundled.naming_schemes.clone(),
    };
    if overrides.format_version != OVERRIDES_FORMAT_VERSION {
        issues.push(ValidationIssue::new(
            Severity::Error,
            Location::default(),
            IssueKind::UnsupportedSchemaVersion {
                found: overrides.format_version,
                supported: OVERRIDES_FORMAT_VERSION,
            },
        ));
    }

    for base in &bundled.tables {
        let loc = Location::table(&base.id);
        match overrides.tables.get(&base.id) {
            None => {
                origins.tables.insert(base.id.clone(), Origin::Bundled);
                origins.results.insert(
                    base.id.clone(),
                    base.results
                        .iter()
                        .map(|r| (r.id.clone(), Origin::Bundled))
                        .collect(),
                );
                content.tables.push(base.clone());
            }
            Some(TableOverride::Delete) => {
                origins.deleted_tables.insert(base.id.clone());
            }
            Some(TableOverride::Modify(patch)) => {
                if patch.base != Fingerprint::of_table_header(base) {
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        loc,
                        IssueKind::DefaultChanged,
                    ));
                }
                let table = apply_table_patch(base, patch, &mut origins, &mut issues);
                origins.tables.insert(base.id.clone(), Origin::Modified);
                content.tables.push(table);
            }
            Some(TableOverride::Custom(_)) => {
                // A custom table cannot replace a default: keep the default.
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    loc,
                    IssueKind::DuplicateId {
                        scope: IdScope::Table,
                        id: base.id.to_string(),
                    },
                ));
                origins.tables.insert(base.id.clone(), Origin::Bundled);
                content.tables.push(base.clone());
            }
        }
    }
    for (id, entry) in &overrides.tables {
        if bundled.table(id).is_some() {
            continue;
        }
        let loc = Location::table(id);
        match entry {
            TableOverride::Custom(table) => {
                let mut table = table.clone();
                table.id = id.clone();
                if !id.is_user() {
                    issues.push(ValidationIssue::new(
                        Severity::Error,
                        loc,
                        IssueKind::NotUserId { id: id.to_string() },
                    ));
                }
                origins.tables.insert(id.clone(), Origin::Custom);
                origins.results.insert(
                    id.clone(),
                    table
                        .results
                        .iter()
                        .map(|r| (r.id.clone(), Origin::Custom))
                        .collect(),
                );
                content.tables.push(table);
            }
            TableOverride::Modify(_) | TableOverride::Delete => issues.push(ValidationIssue::new(
                Severity::Warning,
                loc,
                IssueKind::OverrideTargetMissing,
            )),
        }
    }

    for base in &bundled.tags {
        let loc = Location::tag(&base.id);
        match overrides.tags.get(&base.id) {
            None => {
                origins.tags.insert(base.id.clone(), Origin::Bundled);
                content.tags.push(base.clone());
            }
            Some(TagOverride::Delete) => {
                origins.deleted_tags.insert(base.id.clone());
            }
            Some(TagOverride::Modify(patch)) => {
                if patch.base != Fingerprint::of(base) {
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        loc,
                        IssueKind::DefaultChanged,
                    ));
                }
                let mut tag = base.clone();
                apply_field(&patch.name, &mut tag.name);
                apply_field(&patch.description, &mut tag.description);
                apply_field(&patch.effects, &mut tag.effects);
                apply_field(&patch.enabled, &mut tag.enabled);
                origins.tags.insert(base.id.clone(), Origin::Modified);
                content.tags.push(tag);
            }
            Some(TagOverride::Custom(_)) => {
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    loc,
                    IssueKind::DuplicateId {
                        scope: IdScope::Tag,
                        id: base.id.to_string(),
                    },
                ));
                origins.tags.insert(base.id.clone(), Origin::Bundled);
                content.tags.push(base.clone());
            }
        }
    }
    for (id, entry) in &overrides.tags {
        if bundled.tag(id).is_some() {
            continue;
        }
        let loc = Location::tag(id);
        match entry {
            TagOverride::Custom(tag) => {
                let mut tag = tag.clone();
                tag.id = id.clone();
                if !id.is_user() {
                    issues.push(ValidationIssue::new(
                        Severity::Error,
                        loc,
                        IssueKind::NotUserId { id: id.to_string() },
                    ));
                }
                origins.tags.insert(id.clone(), Origin::Custom);
                content.tags.push(tag);
            }
            TagOverride::Modify(_) | TagOverride::Delete => issues.push(ValidationIssue::new(
                Severity::Warning,
                loc,
                IssueKind::OverrideTargetMissing,
            )),
        }
    }

    issues.extend(validate(&content).issues);
    MergedContent {
        content,
        origins,
        report: ValidationReport { issues },
    }
}

fn apply_table_patch(
    base: &Table,
    patch: &TablePatch,
    origins: &mut Origins,
    issues: &mut Vec<ValidationIssue>,
) -> Table {
    let mut table = base.clone();
    apply_field(&patch.name, &mut table.name);
    apply_field(&patch.description, &mut table.description);
    apply_field(&patch.dice, &mut table.dice);
    apply_field(&patch.active, &mut table.active);
    apply_field(&patch.eligibility, &mut table.eligibility);
    apply_field(&patch.variants, &mut table.variants);
    apply_field(&patch.follow_ups, &mut table.follow_ups);

    let loc = Location::table(&base.id);
    let mut result_origins = BTreeMap::new();
    table.results.clear();
    for result in &base.results {
        match patch.results.get(&result.id) {
            None => {
                result_origins.insert(result.id.clone(), Origin::Bundled);
                table.results.push(result.clone());
            }
            Some(ResultOverride::Delete) => {
                origins
                    .deleted_results
                    .entry(base.id.clone())
                    .or_default()
                    .insert(result.id.clone());
            }
            Some(ResultOverride::Modify(rp)) => {
                if rp.base != Fingerprint::of(result) {
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        loc.clone().with_result(&result.id),
                        IssueKind::DefaultChanged,
                    ));
                }
                let mut result = result.clone();
                rp.apply(&mut result);
                result_origins.insert(result.id.clone(), Origin::Modified);
                table.results.push(result);
            }
        }
    }
    for id in patch.results.keys() {
        if base.result(id).is_none() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                loc.clone().with_result(id),
                IssueKind::OverrideTargetMissing,
            ));
        }
    }
    for added in &patch.added_results {
        if !added.id.is_user() {
            issues.push(ValidationIssue::new(
                Severity::Error,
                loc.clone().with_result(&added.id),
                IssueKind::NotUserId {
                    id: added.id.to_string(),
                },
            ));
        }
        result_origins
            .entry(added.id.clone())
            .or_insert(Origin::Custom);
        table.results.push(added.clone());
    }
    origins.results.insert(base.id.clone(), result_origins);
    table
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("table `{0}` does not exist")]
    UnknownTable(TableId),
    #[error("table `{table}` has no result `{result}`")]
    UnknownResult { table: TableId, result: ResultId },
    #[error("tag `{0}` does not exist")]
    UnknownTag(TagId),
    #[error("`{0}` has been deleted; restore it to its default first")]
    Deleted(String),
    #[error("ID `{0}` is already in use")]
    IdInUse(String),
    #[error("user-created ID `{0}` must start with `{USER_ID_PREFIX}`")]
    NotUserId(String),
    #[error("`{0}` is not a bundled default, so it has no default to restore")]
    NotBundled(String),
}

/// A unique ID in the user namespace, e.g. `user.table.0192f3a4c1d2-7f3e09a1`.
/// The time prefix keeps user-created items in creation order.
pub fn new_user_id(kind: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let suffix: u32 = rand::random();
    format!("{USER_ID_PREFIX}{kind}.{millis:012x}-{suffix:08x}")
}

enum TableEdit<'a> {
    Custom(&'a mut Table),
    Patch {
        base: &'a Table,
        patch: &'a mut TablePatch,
    },
}

// A result: either user-owned or a patch over a default.
enum ResultEdit<'a> {
    Custom(&'a mut TableResult),
    Patch(&'a mut ResultPatch),
}

enum TagEdit<'a> {
    Custom(&'a mut Tag),
    Patch(&'a mut TagPatch),
}

/// Bundled defaults plus user overrides, with editing operations that only
/// ever change the override layer.
#[derive(Debug, Clone)]
pub struct ContentLayers {
    bundled: ContentSet,
    overrides: UserOverrides,
}

impl ContentLayers {
    pub fn new(bundled: ContentSet, overrides: UserOverrides) -> Self {
        Self { bundled, overrides }
    }

    pub fn bundled(&self) -> &ContentSet {
        &self.bundled
    }

    pub fn overrides(&self) -> &UserOverrides {
        &self.overrides
    }

    pub fn into_overrides(self) -> UserOverrides {
        self.overrides
    }

    pub fn merged(&self) -> MergedContent {
        merge(&self.bundled, &self.overrides)
    }

    // ----- tables -----------------------------------------------------------

    pub fn rename_table(&mut self, id: &TableId, name: impl Into<String>) -> Result<(), EditError> {
        let name = name.into();
        self.edit_table(id, |e| match e {
            TableEdit::Custom(t) => t.name = name,
            TableEdit::Patch { patch, .. } => patch.name = Some(name),
        })
    }

    pub fn set_table_description(
        &mut self,
        id: &TableId,
        description: impl Into<String>,
    ) -> Result<(), EditError> {
        let description = description.into();
        self.edit_table(id, |e| match e {
            TableEdit::Custom(t) => t.description = description,
            TableEdit::Patch { patch, .. } => patch.description = Some(description),
        })
    }

    /// Stores the expression as given; invalid dice surface as validation
    /// errors on the merged content.
    pub fn set_table_dice(
        &mut self,
        id: &TableId,
        dice: impl Into<String>,
    ) -> Result<(), EditError> {
        let dice = dice.into();
        self.edit_table(id, |e| match e {
            TableEdit::Custom(t) => t.dice = Some(dice),
            TableEdit::Patch { patch, .. } => patch.dice = Some(Some(dice)),
        })
    }

    pub fn set_table_active(&mut self, id: &TableId, active: bool) -> Result<(), EditError> {
        self.edit_table(id, |e| match e {
            TableEdit::Custom(t) => t.active = active,
            TableEdit::Patch { patch, .. } => patch.active = Some(active),
        })
    }

    pub fn set_table_eligibility(
        &mut self,
        id: &TableId,
        eligibility: Option<Condition>,
    ) -> Result<(), EditError> {
        self.edit_table(id, |e| match e {
            TableEdit::Custom(t) => t.eligibility = eligibility,
            TableEdit::Patch { patch, .. } => patch.eligibility = Some(eligibility),
        })
    }

    /// Adds a user-created table. Its ID must be in the `user.` namespace
    /// (see [`new_user_id`]) and unused.
    pub fn add_table(&mut self, table: Table) -> Result<(), EditError> {
        let id = table.id.clone();
        if !id.is_user() {
            return Err(EditError::NotUserId(id.to_string()));
        }
        if self.bundled.table(&id).is_some() || self.overrides.tables.contains_key(&id) {
            return Err(EditError::IdInUse(id.to_string()));
        }
        self.overrides
            .tables
            .insert(id, TableOverride::Custom(table));
        Ok(())
    }

    /// Deletes a table: user-created tables are removed; bundled tables are
    /// marked deleted in the override layer (discarding customisations).
    pub fn delete_table(&mut self, id: &TableId) -> Result<(), EditError> {
        if self.bundled.table(id).is_some() {
            self.overrides
                .tables
                .insert(id.clone(), TableOverride::Delete);
            Ok(())
        } else if matches!(
            self.overrides.tables.get(id),
            Some(TableOverride::Custom(_))
        ) {
            self.overrides.tables.remove(id);
            Ok(())
        } else {
            Err(EditError::UnknownTable(id.clone()))
        }
    }

    /// Restores a bundled table, including all its results, to the installed
    /// default (undoing deletion and every customisation).
    pub fn reset_table(&mut self, id: &TableId) -> Result<(), EditError> {
        if self.bundled.table(id).is_none() {
            return Err(EditError::NotBundled(id.to_string()));
        }
        self.overrides.tables.remove(id);
        Ok(())
    }

    // ----- results ----------------------------------------------------------

    pub fn rename_result(
        &mut self,
        table: &TableId,
        result: &ResultId,
        name: impl Into<String>,
    ) -> Result<(), EditError> {
        let name = name.into();
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.name = name,
            ResultEdit::Patch(patch) => patch.name = Some(name),
        })
    }

    pub fn set_result_description(
        &mut self,
        table: &TableId,
        result: &ResultId,
        description: impl Into<String>,
    ) -> Result<(), EditError> {
        let description = description.into();
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.description = description,
            ResultEdit::Patch(patch) => patch.description = Some(description),
        })
    }

    pub fn set_result_range(
        &mut self,
        table: &TableId,
        result: &ResultId,
        range: Option<FaceRange>,
    ) -> Result<(), EditError> {
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.range = range,
            ResultEdit::Patch(patch) => patch.range = Some(range),
        })
    }

    pub fn set_result_tags(
        &mut self,
        table: &TableId,
        result: &ResultId,
        tags: Vec<TagId>,
    ) -> Result<(), EditError> {
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.tags = tags,
            ResultEdit::Patch(patch) => patch.tags = Some(tags),
        })
    }

    pub fn set_result_eligibility(
        &mut self,
        table: &TableId,
        result: &ResultId,
        eligibility: Option<Condition>,
    ) -> Result<(), EditError> {
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.eligibility = eligibility,
            ResultEdit::Patch(patch) => patch.eligibility = Some(eligibility),
        })
    }

    pub fn set_result_enabled(
        &mut self,
        table: &TableId,
        result: &ResultId,
        enabled: bool,
    ) -> Result<(), EditError> {
        self.edit_result(table, result, |e| match e {
            ResultEdit::Custom(r) => r.enabled = enabled,
            ResultEdit::Patch(patch) => patch.enabled = Some(enabled),
        })
    }

    /// Adds a user-created result to any table. Its ID must be in the
    /// `user.` namespace and unused in that table.
    pub fn add_result(&mut self, table: &TableId, result: TableResult) -> Result<(), EditError> {
        if !result.id.is_user() {
            return Err(EditError::NotUserId(result.id.to_string()));
        }
        let in_use = EditError::IdInUse(result.id.to_string());
        self.edit_table(table, |e| match e {
            TableEdit::Custom(t) => {
                if t.result(&result.id).is_some() {
                    return Err(in_use);
                }
                t.results.push(result);
                Ok(())
            }
            TableEdit::Patch { base, patch } => {
                let taken = base.result(&result.id).is_some()
                    || patch.added_results.iter().any(|r| r.id == result.id);
                if taken {
                    return Err(in_use);
                }
                patch.added_results.push(result);
                Ok(())
            }
        })?
    }

    /// Deletes a result: user-created results are removed; bundled results
    /// are marked deleted in the override layer.
    pub fn delete_result(&mut self, table: &TableId, result: &ResultId) -> Result<(), EditError> {
        let unknown = EditError::UnknownResult {
            table: table.clone(),
            result: result.clone(),
        };
        self.edit_table(table, |e| match e {
            TableEdit::Custom(t) => {
                let before = t.results.len();
                t.results.retain(|r| &r.id != result);
                if t.results.len() == before {
                    Err(unknown)
                } else {
                    Ok(())
                }
            }
            TableEdit::Patch { base, patch } => {
                if base.result(result).is_some() {
                    patch.results.insert(result.clone(), ResultOverride::Delete);
                    Ok(())
                } else if let Some(pos) = patch.added_results.iter().position(|r| &r.id == result) {
                    patch.added_results.remove(pos);
                    Ok(())
                } else {
                    Err(unknown)
                }
            }
        })?
    }

    /// Restores one bundled result to the installed default (undoing deletion
    /// and customisation), leaving the rest of the table as it is.
    pub fn reset_result(&mut self, table: &TableId, result: &ResultId) -> Result<(), EditError> {
        let Some(base) = self.bundled.table(table) else {
            return Err(EditError::NotBundled(table.to_string()));
        };
        if base.result(result).is_none() {
            return Err(EditError::NotBundled(result.to_string()));
        }
        if let Some(TableOverride::Modify(patch)) = self.overrides.tables.get_mut(table) {
            patch.results.remove(result);
            if patch.is_empty() {
                self.overrides.tables.remove(table);
            }
        }
        Ok(())
    }

    // ----- tags -------------------------------------------------------------

    pub fn rename_tag(&mut self, id: &TagId, name: impl Into<String>) -> Result<(), EditError> {
        let name = name.into();
        self.edit_tag(id, |e| match e {
            TagEdit::Custom(t) => t.name = name,
            TagEdit::Patch(p) => p.name = Some(name),
        })
    }

    pub fn set_tag_description(
        &mut self,
        id: &TagId,
        description: impl Into<String>,
    ) -> Result<(), EditError> {
        let description = description.into();
        self.edit_tag(id, |e| match e {
            TagEdit::Custom(t) => t.description = description,
            TagEdit::Patch(p) => p.description = Some(description),
        })
    }

    pub fn set_tag_effects(
        &mut self,
        id: &TagId,
        effects: Vec<EffectDef>,
    ) -> Result<(), EditError> {
        self.edit_tag(id, |e| match e {
            TagEdit::Custom(t) => t.effects = effects,
            TagEdit::Patch(p) => p.effects = Some(effects),
        })
    }

    pub fn set_tag_enabled(&mut self, id: &TagId, enabled: bool) -> Result<(), EditError> {
        self.edit_tag(id, |e| match e {
            TagEdit::Custom(t) => t.enabled = enabled,
            TagEdit::Patch(p) => p.enabled = Some(enabled),
        })
    }

    pub fn add_tag(&mut self, tag: Tag) -> Result<(), EditError> {
        let id = tag.id.clone();
        if !id.is_user() {
            return Err(EditError::NotUserId(id.to_string()));
        }
        if self.bundled.tag(&id).is_some() || self.overrides.tags.contains_key(&id) {
            return Err(EditError::IdInUse(id.to_string()));
        }
        self.overrides.tags.insert(id, TagOverride::Custom(tag));
        Ok(())
    }

    pub fn delete_tag(&mut self, id: &TagId) -> Result<(), EditError> {
        if self.bundled.tag(id).is_some() {
            self.overrides.tags.insert(id.clone(), TagOverride::Delete);
            Ok(())
        } else if matches!(self.overrides.tags.get(id), Some(TagOverride::Custom(_))) {
            self.overrides.tags.remove(id);
            Ok(())
        } else {
            Err(EditError::UnknownTag(id.clone()))
        }
    }

    pub fn reset_tag(&mut self, id: &TagId) -> Result<(), EditError> {
        if self.bundled.tag(id).is_none() {
            return Err(EditError::NotBundled(id.to_string()));
        }
        self.overrides.tags.remove(id);
        Ok(())
    }

    // ----- everything -------------------------------------------------------

    /// Restores all generator content to the installed defaults, discarding
    /// every customisation and every user-created table, result and tag.
    pub fn reset_all(&mut self) {
        self.overrides = UserOverrides::default();
    }

    // ----- whole-item edits (editors) ---------------------------------------

    /// Stores `table` as the effective version of its ID. Creates a custom
    /// table for a new `user.` ID. For a bundled table, records only the
    /// fields that differ from the default (so later default updates still
    /// flow through), deletes bundled results that were removed, and appends
    /// added results (which need `user.` IDs). Bundled result order, the
    /// selection mode and the content status cannot be changed.
    pub fn put_table(&mut self, table: Table) -> Result<(), EditError> {
        let id = table.id.clone();
        let Some(base) = self.bundled.table(&id) else {
            return match self.overrides.tables.get_mut(&id) {
                Some(TableOverride::Custom(existing)) => {
                    *existing = table;
                    Ok(())
                }
                Some(_) => Err(EditError::UnknownTable(id)),
                None => self.add_table(table),
            };
        };
        let previous = match self.overrides.tables.get(&id) {
            Some(TableOverride::Modify(patch)) => Some(patch.clone()),
            _ => None,
        };
        let mut patch = previous.clone().unwrap_or_else(|| TablePatch::new(base));
        patch.name = Some(table.name.clone());
        patch.description = Some(table.description.clone());
        patch.dice = Some(table.dice.clone());
        patch.active = Some(table.active);
        patch.eligibility = Some(table.eligibility.clone());
        patch.variants = Some(table.variants.clone());
        patch.follow_ups = Some(table.follow_ups.clone());

        let mut results = BTreeMap::new();
        for base_result in &base.results {
            match table.result(&base_result.id) {
                None => {
                    results.insert(base_result.id.clone(), ResultOverride::Delete);
                }
                Some(result) => {
                    let mut rp = match previous.as_ref().and_then(|p| p.results.get(&result.id)) {
                        Some(ResultOverride::Modify(rp)) => rp.clone(),
                        _ => ResultPatch::new(base_result),
                    };
                    rp.name = Some(result.name.clone());
                    rp.description = Some(result.description.clone());
                    rp.range = Some(result.range);
                    rp.tags = Some(result.tags.clone());
                    rp.eligibility = Some(result.eligibility.clone());
                    rp.enabled = Some(result.enabled);
                    rp.value = Some(result.value);
                    rp.prune(base_result);
                    if !rp.is_empty() {
                        results.insert(result.id.clone(), ResultOverride::Modify(rp));
                    }
                }
            }
        }
        let mut added = Vec::new();
        for result in &table.results {
            if base.result(&result.id).is_none() {
                if !result.id.is_user() {
                    return Err(EditError::NotUserId(result.id.to_string()));
                }
                added.push(result.clone());
            }
        }
        patch.results = results;
        patch.added_results = added;
        patch.prune(base);
        if patch.is_empty() {
            self.overrides.tables.remove(&id);
        } else {
            self.overrides
                .tables
                .insert(id, TableOverride::Modify(patch));
        }
        Ok(())
    }

    /// Stores `tag` as the effective version of its ID (see [`put_table`]).
    ///
    /// [`put_table`]: Self::put_table
    pub fn put_tag(&mut self, tag: Tag) -> Result<(), EditError> {
        let id = tag.id.clone();
        let Some(base) = self.bundled.tag(&id) else {
            return match self.overrides.tags.get_mut(&id) {
                Some(TagOverride::Custom(existing)) => {
                    *existing = tag;
                    Ok(())
                }
                Some(_) => Err(EditError::UnknownTag(id)),
                None => self.add_tag(tag),
            };
        };
        let mut patch = match self.overrides.tags.get(&id) {
            Some(TagOverride::Modify(patch)) => patch.clone(),
            _ => TagPatch::new(base),
        };
        patch.name = Some(tag.name);
        patch.description = Some(tag.description);
        patch.effects = Some(tag.effects);
        patch.enabled = Some(tag.enabled);
        patch.prune(base);
        if patch.is_empty() {
            self.overrides.tags.remove(&id);
        } else {
            self.overrides.tags.insert(id, TagOverride::Modify(patch));
        }
        Ok(())
    }

    /// Applies `edit` only if it introduces no new validation errors
    /// anywhere in the merged content. Otherwise nothing changes and the new
    /// errors are returned, so malformed content never reaches generation.
    pub fn apply_checked<R>(
        &mut self,
        edit: impl FnOnce(&mut ContentLayers) -> Result<R, EditError>,
    ) -> Result<R, CheckedEditError> {
        let (trial, result) = self.preview(edit)?;
        let new_errors = new_errors(&self.merged().report, &trial.merged().report);
        if !new_errors.is_empty() {
            return Err(CheckedEditError::Invalid(new_errors));
        }
        *self = trial;
        Ok(result)
    }

    /// Applies `edit` to a copy, leaving `self` unchanged.
    pub fn preview<R>(
        &self,
        edit: impl FnOnce(&mut ContentLayers) -> Result<R, EditError>,
    ) -> Result<(ContentLayers, R), EditError> {
        let mut trial = self.clone();
        let result = edit(&mut trial)?;
        Ok((trial, result))
    }

    // ----- helpers ----------------------------------------------------------

    fn edit_table<R>(
        &mut self,
        id: &TableId,
        f: impl FnOnce(TableEdit<'_>) -> R,
    ) -> Result<R, EditError> {
        let Some(base) = self.bundled.table(id) else {
            return match self.overrides.tables.get_mut(id) {
                Some(TableOverride::Custom(t)) => Ok(f(TableEdit::Custom(t))),
                _ => Err(EditError::UnknownTable(id.clone())),
            };
        };
        let entry = self
            .overrides
            .tables
            .entry(id.clone())
            .or_insert_with(|| TableOverride::Modify(TablePatch::new(base)));
        let patch = match entry {
            TableOverride::Modify(patch) => patch,
            TableOverride::Delete => return Err(EditError::Deleted(id.to_string())),
            TableOverride::Custom(_) => return Err(EditError::IdInUse(id.to_string())),
        };
        let out = f(TableEdit::Patch { base, patch });
        patch.prune(base);
        if patch.is_empty() {
            self.overrides.tables.remove(id);
        }
        Ok(out)
    }

    fn edit_result<R>(
        &mut self,
        table: &TableId,
        result: &ResultId,
        f: impl FnOnce(ResultEdit<'_>) -> R,
    ) -> Result<R, EditError> {
        let unknown = || EditError::UnknownResult {
            table: table.clone(),
            result: result.clone(),
        };
        self.edit_table(table, |e| match e {
            TableEdit::Custom(t) => t
                .results
                .iter_mut()
                .find(|r| &r.id == result)
                .map(|r| f(ResultEdit::Custom(r)))
                .ok_or_else(unknown),
            TableEdit::Patch { base, patch } => {
                if let Some(base_result) = base.result(result) {
                    let entry = patch
                        .results
                        .entry(result.clone())
                        .or_insert_with(|| ResultOverride::Modify(ResultPatch::new(base_result)));
                    let ResultOverride::Modify(rp) = entry else {
                        return Err(EditError::Deleted(result.to_string()));
                    };
                    let out = f(ResultEdit::Patch(rp));
                    rp.prune(base_result);
                    if rp.is_empty() {
                        patch.results.remove(result);
                    }
                    Ok(out)
                } else {
                    patch
                        .added_results
                        .iter_mut()
                        .find(|r| &r.id == result)
                        .map(|r| f(ResultEdit::Custom(r)))
                        .ok_or_else(unknown)
                }
            }
        })?
    }

    fn edit_tag<R>(
        &mut self,
        id: &TagId,
        f: impl FnOnce(TagEdit<'_>) -> R,
    ) -> Result<R, EditError> {
        let Some(base) = self.bundled.tag(id) else {
            return match self.overrides.tags.get_mut(id) {
                Some(TagOverride::Custom(t)) => Ok(f(TagEdit::Custom(t))),
                _ => Err(EditError::UnknownTag(id.clone())),
            };
        };
        let entry = self
            .overrides
            .tags
            .entry(id.clone())
            .or_insert_with(|| TagOverride::Modify(TagPatch::new(base)));
        let patch = match entry {
            TagOverride::Modify(patch) => patch,
            TagOverride::Delete => return Err(EditError::Deleted(id.to_string())),
            TagOverride::Custom(_) => return Err(EditError::IdInUse(id.to_string())),
        };
        let out = f(TagEdit::Patch(patch));
        patch.prune(base);
        if patch.is_empty() {
            self.overrides.tags.remove(id);
        }
        Ok(out)
    }
}

/// Serde helper distinguishing "field absent" (`None`) from "explicitly
/// cleared" (`Some(None)`, written as `null`).
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        value: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

/// Why a checked edit was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckedEditError {
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error("the change would make the generator content invalid:\n{}", format_issues(.0))]
    Invalid(Vec<ValidationIssue>),
}

fn format_issues(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(|i| format!("- {}", i.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Errors in `after` that were not already in `before`.
pub fn new_errors(before: &ValidationReport, after: &ValidationReport) -> Vec<ValidationIssue> {
    let existing: std::collections::HashSet<&str> =
        before.errors().map(|i| i.message.as_str()).collect();
    after
        .errors()
        .filter(|i| !existing.contains(i.message.as_str()))
        .cloned()
        .collect()
}
