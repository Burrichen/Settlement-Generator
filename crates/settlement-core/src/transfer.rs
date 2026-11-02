//! Import and export of content: single tables, tags, settings and flows,
//! or a complete content pack, as a readable JSON file. Every reference is
//! checked before anything is imported, and an ID that is already in use is
//! handled the way the user chooses (a new ID, replacing, or skipping),
//! with references inside the pack rewritten to match.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::content::{ContentSet, Table, Tag};
use crate::framework::layers::FrameworkItem;
use crate::framework::{Flow, ObjectType, Scale, SettingDef, SharedResult};
use crate::layers::{ContentLayers, EditError, new_user_id};
use crate::validation::{Severity, ValidationIssue};

/// The `format` of a content pack file.
pub const PACK_FORMAT: &str = "settlement-generator-content";
/// The newest pack version this build reads and writes.
pub const PACK_VERSION: u32 = 1;

/// Content as a file: items of every kind, each complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentPack {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub app_version: String,
    /// What the pack is, e.g. "Crime table" or "All my content".
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scales: Vec<Scale>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_results: Vec<SharedResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<Table>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<Flow>,
}

/// The kinds of item a pack carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    Setting,
    Scale,
    SharedResult,
    ObjectType,
    Table,
    Tag,
    Flow,
}

impl PackKind {
    pub fn label(self) -> &'static str {
        match self {
            PackKind::Setting => "Setting",
            PackKind::Scale => "Scale",
            PackKind::SharedResult => "Shared result",
            PackKind::ObjectType => "Object type",
            PackKind::Table => "Table",
            PackKind::Tag => "Tag",
            PackKind::Flow => "Flow",
        }
    }

    /// The word used in new user IDs.
    fn id_word(self) -> &'static str {
        match self {
            PackKind::Setting => "setting",
            PackKind::Scale => "scale",
            PackKind::SharedResult => "shared",
            PackKind::ObjectType => "object",
            PackKind::Table => "table",
            PackKind::Tag => "tag",
            PackKind::Flow => "flow",
        }
    }
}

/// One item, by kind and ID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PackItem {
    pub kind: PackKind,
    pub id: String,
}

/// What to do with an imported item whose ID is already in use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collision {
    /// Import it as a new item with a new ID (the safe default).
    #[default]
    Rename,
    /// Replace the existing item (a default becomes customised).
    Replace,
    /// Keep the existing item; the pack's references point to it.
    Skip,
}

/// What importing one item will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportAction {
    /// A new item.
    Add,
    /// Added under a new ID (its ID is taken, or is not a user ID).
    AddAs { id: String },
    /// Replaces the existing item of that ID.
    Replace { existing: String },
    /// Not imported: the existing item is kept.
    Skip { existing: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    pub kind: PackKind,
    pub id: String,
    pub name: String,
    pub action: ImportAction,
}

/// What an import would do, checked against the current content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportPlan {
    pub name: String,
    pub items: Vec<ImportItem>,
    /// Errors the import would cause (missing references and the like).
    pub issues: Vec<ValidationIssue>,
    /// Why the file cannot be imported at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub can_import: bool,
    pub summary: String,
}

fn kinds_of(pack: &ContentPack) -> Vec<(PackKind, String, String)> {
    let mut out = Vec::new();
    out.extend(
        pack.settings
            .iter()
            .map(|i| (PackKind::Setting, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.scales
            .iter()
            .map(|i| (PackKind::Scale, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.shared_results
            .iter()
            .map(|i| (PackKind::SharedResult, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.objects
            .iter()
            .map(|i| (PackKind::ObjectType, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.tables
            .iter()
            .map(|i| (PackKind::Table, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.tags
            .iter()
            .map(|i| (PackKind::Tag, i.id.to_string(), i.name.clone())),
    );
    out.extend(
        pack.flows
            .iter()
            .map(|i| (PackKind::Flow, i.id.to_string(), i.name.clone())),
    );
    out
}

/// Every top-level item of `content`, by kind and ID, with its name.
fn items_of(content: &ContentSet) -> BTreeMap<(PackKind, String), String> {
    let mut out = BTreeMap::new();
    for s in &content.settings {
        out.insert((PackKind::Setting, s.id.to_string()), s.name.clone());
    }
    for s in &content.scales {
        out.insert((PackKind::Scale, s.id.to_string()), s.name.clone());
    }
    for s in &content.shared_results {
        out.insert((PackKind::SharedResult, s.id.to_string()), s.name.clone());
    }
    for o in &content.objects {
        out.insert((PackKind::ObjectType, o.id.to_string()), o.name.clone());
    }
    for t in &content.tables {
        out.insert((PackKind::Table, t.id.to_string()), t.name.clone());
    }
    for t in &content.tags {
        out.insert((PackKind::Tag, t.id.to_string()), t.name.clone());
    }
    for f in &content.flows {
        out.insert((PackKind::Flow, f.id.to_string()), f.name.clone());
    }
    out
}

/// Every string anywhere in `value`.
fn strings(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(a) => a.iter().for_each(|v| strings(v, out)),
        Value::Object(o) => o.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

/// `value` with every string equal to a key of `renames` replaced.
fn rename_strings(value: &mut Value, renames: &BTreeMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(new) = renames.get(s.as_str()) {
                *s = new.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| rename_strings(v, renames)),
        Value::Object(o) => o.values_mut().for_each(|v| rename_strings(v, renames)),
        _ => {}
    }
}

fn item_json(content: &ContentSet, item: &PackItem) -> Option<Value> {
    let find = |v: Option<Value>| v;
    find(match item.kind {
        PackKind::Setting => content
            .settings
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::Scale => content
            .scales
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::SharedResult => content
            .shared_results
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::ObjectType => content
            .objects
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::Table => content
            .tables
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::Tag => content
            .tags
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
        PackKind::Flow => content
            .flows
            .iter()
            .find(|s| s.id.as_str() == item.id)
            .map(|s| serde_json::to_value(s).ok())?,
    })
}

/// The other items `item` names (tables it rolls, tags it checks, flows it
/// runs, settings it tests …), directly.
fn references(content: &ContentSet, item: &PackItem) -> Vec<PackItem> {
    let Some(json) = item_json(content, item) else {
        return Vec::new();
    };
    let mut found = BTreeSet::new();
    strings(&json, &mut found);
    items_of(content)
        .into_keys()
        .filter(|(kind, id)| found.contains(id) && !(*kind == item.kind && *id == item.id))
        .map(|(kind, id)| PackItem { kind, id })
        .collect()
}

/// `items`, and with `with_dependencies` everything they name in turn.
/// A dependency that is an unchanged installed default (the same in
/// `bundled`) is left out: every installation has it, and the pack names
/// it by ID.
pub fn export(
    content: &ContentSet,
    bundled: Option<&ContentSet>,
    items: &[PackItem],
    with_dependencies: bool,
    name: &str,
) -> Result<ContentPack, String> {
    let mut chosen: BTreeSet<PackItem> = items.iter().cloned().collect();
    let mut seen: BTreeSet<PackItem> = BTreeSet::new();
    let mut queue: Vec<PackItem> = items.to_vec();
    while let Some(item) = queue.pop() {
        let Some(json) = item_json(content, &item) else {
            return Err(format!(
                "There is no {} `{}`.",
                item.kind.label().to_lowercase(),
                item.id
            ));
        };
        if !seen.insert(item.clone()) {
            continue;
        }
        let unchanged_default =
            bundled.is_some_and(|b| item_json(b, &item).as_ref() == Some(&json));
        if !unchanged_default {
            chosen.insert(item.clone());
        }
        if with_dependencies {
            queue.extend(references(content, &item));
        }
    }
    let has = |kind: PackKind, id: &str| {
        chosen.contains(&PackItem {
            kind,
            id: id.to_owned(),
        })
    };
    Ok(ContentPack {
        format: PACK_FORMAT.into(),
        version: PACK_VERSION,
        app_version: crate::VERSION.into(),
        name: name.into(),
        settings: content
            .settings
            .iter()
            .filter(|i| has(PackKind::Setting, i.id.as_str()))
            .cloned()
            .collect(),
        scales: content
            .scales
            .iter()
            .filter(|i| has(PackKind::Scale, i.id.as_str()))
            .cloned()
            .collect(),
        shared_results: content
            .shared_results
            .iter()
            .filter(|i| has(PackKind::SharedResult, i.id.as_str()))
            .cloned()
            .collect(),
        objects: content
            .objects
            .iter()
            .filter(|i| has(PackKind::ObjectType, i.id.as_str()))
            .cloned()
            .collect(),
        tables: content
            .tables
            .iter()
            .filter(|i| has(PackKind::Table, i.id.as_str()))
            .cloned()
            .collect(),
        tags: content
            .tags
            .iter()
            .filter(|i| has(PackKind::Tag, i.id.as_str()))
            .cloned()
            .collect(),
        flows: content
            .flows
            .iter()
            .filter(|i| has(PackKind::Flow, i.id.as_str()))
            .cloned()
            .collect(),
    })
}

/// Every item of `content` as one pack (a complete content pack).
pub fn export_all(content: &ContentSet, name: &str) -> ContentPack {
    let items: Vec<PackItem> = items_of(content)
        .into_keys()
        .map(|(kind, id)| PackItem { kind, id })
        .collect();
    export(content, None, &items, false, name).expect("every item exists")
}

/// Reads a pack file, refusing other files and newer formats.
pub fn read_pack(text: &str) -> Result<ContentPack, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("The file is not valid JSON: {e}"))?;
    if value.get("format").and_then(Value::as_str) != Some(PACK_FORMAT) {
        return Err("This is not a Settlement Generator content file.".into());
    }
    let version = value.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > u64::from(PACK_VERSION) {
        return Err(format!(
            "This file was made by a newer version (content format {version}); update the app to import it."
        ));
    }
    serde_json::from_value(value).map_err(|e| format!("The file could not be read: {e}"))
}

/// The pack as it will be imported: IDs decided and references rewritten,
/// with what will happen to each item.
fn resolve(
    layers: &ContentLayers,
    pack: &ContentPack,
    policy: Collision,
) -> (ContentPack, Vec<ImportItem>) {
    let current = items_of(&layers.merged().content);
    let bundled = items_of(layers.bundled());
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    let mut skipped: BTreeSet<(PackKind, String)> = BTreeSet::new();
    let mut plan = Vec::new();
    for (kind, id, name) in kinds_of(pack) {
        let key = (kind, id.clone());
        let user = id.starts_with(crate::ids::USER_ID_PREFIX);
        let mut add_as = |id: &str| {
            let new = new_user_id(kind.id_word());
            renames.insert(id.to_owned(), new.clone());
            ImportAction::AddAs { id: new }
        };
        let action = match current.get(&key) {
            Some(existing) => match policy {
                Collision::Skip => {
                    skipped.insert(key);
                    ImportAction::Skip {
                        existing: existing.clone(),
                    }
                }
                Collision::Replace => ImportAction::Replace {
                    existing: existing.clone(),
                },
                Collision::Rename => add_as(&id),
            },
            // Only a user ID or a default's ID can be stored as it is.
            None if user || bundled.contains_key(&key) => ImportAction::Add,
            None => add_as(&id),
        };
        plan.push(ImportItem {
            kind,
            id,
            name,
            action,
        });
    }
    let mut value = serde_json::to_value(pack).expect("packs serialise");
    rename_strings(&mut value, &renames);
    let mut resolved: ContentPack =
        serde_json::from_value(value).expect("renaming keeps the shape");
    // Skipped items are not imported; references keep naming the existing.
    resolved
        .settings
        .retain(|i| !skipped.contains(&(PackKind::Setting, i.id.to_string())));
    resolved
        .scales
        .retain(|i| !skipped.contains(&(PackKind::Scale, i.id.to_string())));
    resolved
        .shared_results
        .retain(|i| !skipped.contains(&(PackKind::SharedResult, i.id.to_string())));
    resolved
        .objects
        .retain(|i| !skipped.contains(&(PackKind::ObjectType, i.id.to_string())));
    resolved
        .tables
        .retain(|i| !skipped.contains(&(PackKind::Table, i.id.to_string())));
    resolved
        .tags
        .retain(|i| !skipped.contains(&(PackKind::Tag, i.id.to_string())));
    resolved
        .flows
        .retain(|i| !skipped.contains(&(PackKind::Flow, i.id.to_string())));
    (resolved, plan)
}

/// Stores every item of `pack` in `layers` (no checking here).
fn put_all(layers: &mut ContentLayers, pack: ContentPack) -> Result<(), EditError> {
    fn items<T: FrameworkItem>(layers: &mut ContentLayers, list: Vec<T>) -> Result<(), EditError> {
        for item in list {
            layers.put_item(item)?;
        }
        Ok(())
    }
    items(layers, pack.settings)?;
    items(layers, pack.scales)?;
    items(layers, pack.shared_results)?;
    items(layers, pack.objects)?;
    for table in pack.tables {
        layers.put_table(table)?;
    }
    for tag in pack.tags {
        layers.put_tag(tag)?;
    }
    items(layers, pack.flows)?;
    Ok(())
}

/// What importing `pack` would do, without changing anything: each item's
/// fate, and every error it would cause (references to things that exist
/// neither in the pack nor in the content, for example).
pub fn plan_import(layers: &ContentLayers, pack: &ContentPack, policy: Collision) -> ImportPlan {
    let (resolved, items) = resolve(layers, pack, policy);
    let before = layers.merged().report;
    let (issues, error) = match layers.preview(|l| put_all(l, resolved)) {
        Ok((trial, ())) => {
            let existing: BTreeSet<&str> = before.errors().map(|i| i.message.as_str()).collect();
            let issues: Vec<ValidationIssue> = trial
                .merged()
                .report
                .issues
                .into_iter()
                .filter(|i| i.severity == Severity::Error && !existing.contains(i.message.as_str()))
                .collect();
            (issues, None)
        }
        Err(e) => (Vec::new(), Some(e.to_string())),
    };
    let count = |f: fn(&ImportAction) -> bool| items.iter().filter(|i| f(&i.action)).count();
    let added = count(|a| matches!(a, ImportAction::Add | ImportAction::AddAs { .. }));
    let replaced = count(|a| matches!(a, ImportAction::Replace { .. }));
    let skipped = count(|a| matches!(a, ImportAction::Skip { .. }));
    let can_import = error.is_none() && issues.is_empty() && !items.is_empty();
    ImportPlan {
        name: pack.name.clone(),
        summary: if items.is_empty() {
            "The file holds no content.".into()
        } else if can_import {
            format!(
                "{added} to add, {replaced} to replace, {skipped} to skip. Every reference checks out."
            )
        } else {
            "It cannot be imported as it is: see the problems below.".into()
        },
        items,
        issues,
        error,
        can_import,
    }
}

/// Imports `pack` into `layers`, refusing (and changing nothing) if any
/// reference would break.
pub fn import(
    layers: &mut ContentLayers,
    pack: &ContentPack,
    policy: Collision,
) -> Result<ImportPlan, String> {
    let plan = plan_import(layers, pack, policy);
    if !plan.can_import {
        let reasons: Vec<&str> = plan.issues.iter().map(|i| i.message.as_str()).collect();
        return Err(plan.error.clone().unwrap_or_else(|| {
            format!(
                "Nothing was imported. It would break references:\n- {}",
                reasons.join("\n- ")
            )
        }));
    }
    let (resolved, _) = resolve(layers, pack, policy);
    // Same IDs as the plan: renames are decided again, but only the IDs
    // differ, and nothing refers to them from outside the pack.
    layers
        .apply_checked(|l| put_all(l, resolved))
        .map_err(|e| e.to_string())?;
    Ok(plan)
}
