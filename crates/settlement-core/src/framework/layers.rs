//! User overrides of framework items: shared results, scales, settings,
//! object types and flows. Each item is customised as a whole: the user's
//! version replaces the bundled one (remembering a fingerprint of the
//! default it replaced), a deleted default is marked deleted, and user
//! items live in the `user.` namespace. Bundled defaults are never changed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::content::ContentSet;
use crate::ids::{FlowId, ObjectTypeId, ScaleId, SettingId, SharedResultId};
use crate::validation::{IdScope, IssueKind, Location, Severity, ValidationIssue};

use super::{Flow, ObjectType, Scale, SettingDef, SharedResult};

/// FNV-1a over canonical JSON, as for table and tag customisations.
pub(crate) fn fingerprint<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("content serialises");
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// A customisation of one framework item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ItemOverride<T> {
    /// An item created by the user.
    Custom { item: T },
    /// The user's version of a bundled item. `base` is the fingerprint of
    /// the default it replaced.
    Replace { base: String, item: T },
    /// A bundled item the user deleted.
    Delete,
}

/// The kinds of framework item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    SharedResult,
    Scale,
    Setting,
    ObjectType,
    Flow,
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ItemKind::SharedResult => "shared result",
            ItemKind::Scale => "scale",
            ItemKind::Setting => "setting",
            ItemKind::ObjectType => "object type",
            ItemKind::Flow => "flow",
        })
    }
}

/// Customisations of every framework item, by kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameworkOverrides {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared_results: BTreeMap<SharedResultId, ItemOverride<SharedResult>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scales: BTreeMap<ScaleId, ItemOverride<Scale>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<SettingId, ItemOverride<SettingDef>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<ObjectTypeId, ItemOverride<ObjectType>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flows: BTreeMap<FlowId, ItemOverride<Flow>>,
}

impl FrameworkOverrides {
    pub fn is_empty(&self) -> bool {
        self.shared_results.is_empty()
            && self.scales.is_empty()
            && self.settings.is_empty()
            && self.objects.is_empty()
            && self.flows.is_empty()
    }
}

/// Where each framework item came from, and which defaults were deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemOrigins {
    pub origins: BTreeMap<String, crate::layers::Origin>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub deleted: BTreeSet<String>,
}

/// A framework item type: how to find it in content and overrides.
pub trait FrameworkItem: Clone + PartialEq + Serialize + DeserializeOwned {
    type Id: Clone + Ord + fmt::Display + Serialize + DeserializeOwned;
    const KIND: ItemKind;
    fn id(&self) -> &Self::Id;
    fn set_id(&mut self, id: Self::Id);
    fn id_str(id: &Self::Id) -> &str;
    fn in_content(content: &ContentSet) -> &Vec<Self>;
    fn in_content_mut(content: &mut ContentSet) -> &mut Vec<Self>;
    fn in_overrides(overrides: &FrameworkOverrides) -> &BTreeMap<Self::Id, ItemOverride<Self>>;
    fn in_overrides_mut(
        overrides: &mut FrameworkOverrides,
    ) -> &mut BTreeMap<Self::Id, ItemOverride<Self>>;
}

macro_rules! framework_item {
    ($ty:ty, $id:ty, $kind:expr, $field:ident) => {
        impl FrameworkItem for $ty {
            type Id = $id;
            const KIND: ItemKind = $kind;
            fn id(&self) -> &$id {
                &self.id
            }
            fn set_id(&mut self, id: $id) {
                self.id = id;
            }
            fn id_str(id: &$id) -> &str {
                id.as_str()
            }
            fn in_content(content: &ContentSet) -> &Vec<Self> {
                &content.$field
            }
            fn in_content_mut(content: &mut ContentSet) -> &mut Vec<Self> {
                &mut content.$field
            }
            fn in_overrides(o: &FrameworkOverrides) -> &BTreeMap<$id, ItemOverride<Self>> {
                &o.$field
            }
            fn in_overrides_mut(
                o: &mut FrameworkOverrides,
            ) -> &mut BTreeMap<$id, ItemOverride<Self>> {
                &mut o.$field
            }
        }
    };
}

framework_item!(
    SharedResult,
    SharedResultId,
    ItemKind::SharedResult,
    shared_results
);
framework_item!(Scale, ScaleId, ItemKind::Scale, scales);
framework_item!(SettingDef, SettingId, ItemKind::Setting, settings);
framework_item!(ObjectType, ObjectTypeId, ItemKind::ObjectType, objects);
framework_item!(Flow, FlowId, ItemKind::Flow, flows);

/// Where issues about an item of kind `kind` are reported.
pub(crate) fn item_location(kind: ItemKind, id: &str) -> Location {
    let mut loc = Location::default();
    match kind {
        ItemKind::SharedResult => loc.shared_result = Some(id.into()),
        ItemKind::Scale => loc.scale = Some(id.into()),
        ItemKind::Setting => loc.setting = Some(id.into()),
        ItemKind::ObjectType => loc.object_type = Some(id.into()),
        ItemKind::Flow => loc.flow = Some(id.into()),
    }
    loc
}

/// The ID scope of a kind, for duplicate-ID issues.
pub(crate) fn id_scope(kind: ItemKind) -> IdScope {
    match kind {
        ItemKind::SharedResult => IdScope::SharedResult,
        ItemKind::Scale => IdScope::Scale,
        ItemKind::Setting => IdScope::Setting,
        ItemKind::ObjectType => IdScope::ObjectType,
        ItemKind::Flow => IdScope::Flow,
    }
}

/// The items of one kind with the user's customisations applied.
pub(crate) fn merge_items<T: FrameworkItem>(
    bundled: &ContentSet,
    overrides: &FrameworkOverrides,
    content: &mut ContentSet,
    origins: &mut BTreeMap<ItemKind, ItemOrigins>,
    issues: &mut Vec<ValidationIssue>,
) {
    use crate::layers::Origin;
    let entries = T::in_overrides(overrides);
    let mut out = Vec::new();
    let record = origins.entry(T::KIND).or_default();
    for base in T::in_content(bundled) {
        let id = T::id_str(base.id()).to_owned();
        let loc = item_location(T::KIND, &id);
        match entries.get(base.id()) {
            None => {
                record.origins.insert(id, Origin::Bundled);
                out.push(base.clone());
            }
            Some(ItemOverride::Delete) => {
                record.deleted.insert(id);
            }
            Some(ItemOverride::Replace { base: print, item }) => {
                if print != &fingerprint(base) {
                    issues.push(ValidationIssue::new(
                        Severity::Warning,
                        loc,
                        IssueKind::DefaultChanged,
                    ));
                }
                let mut item = item.clone();
                item.set_id(base.id().clone());
                record.origins.insert(id, Origin::Modified);
                out.push(item);
            }
            Some(ItemOverride::Custom { .. }) => {
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    loc,
                    IssueKind::DuplicateId {
                        scope: id_scope(T::KIND),
                        id: id.clone(),
                    },
                ));
                record.origins.insert(id, Origin::Bundled);
                out.push(base.clone());
            }
        }
    }
    for (id, entry) in entries {
        if T::in_content(bundled).iter().any(|b| b.id() == id) {
            continue;
        }
        let key = T::id_str(id).to_owned();
        let loc = item_location(T::KIND, &key);
        match entry {
            ItemOverride::Custom { item } => {
                if !key.starts_with(crate::ids::USER_ID_PREFIX) {
                    issues.push(ValidationIssue::new(
                        Severity::Error,
                        loc,
                        IssueKind::NotUserId { id: key.clone() },
                    ));
                }
                let mut item = item.clone();
                item.set_id(id.clone());
                record.origins.insert(key, Origin::Custom);
                out.push(item);
            }
            ItemOverride::Replace { .. } | ItemOverride::Delete => issues.push(
                ValidationIssue::new(Severity::Warning, loc, IssueKind::OverrideTargetMissing),
            ),
        }
    }
    *T::in_content_mut(content) = out;
}

/// Records `item` in `overrides`: as the user's version of a bundled item,
/// or as a user item. Putting a bundled item back exactly as it ships
/// removes the customisation.
pub(crate) fn put_item<T: FrameworkItem>(
    bundled: &ContentSet,
    overrides: &mut FrameworkOverrides,
    item: T,
) -> Result<(), crate::layers::EditError> {
    use crate::layers::EditError;
    let id = item.id().clone();
    let entries = T::in_overrides_mut(overrides);
    match T::in_content(bundled).iter().find(|b| b.id() == &id) {
        Some(base) if base == &item => {
            entries.remove(&id);
        }
        Some(base) => {
            if matches!(entries.get(&id), Some(ItemOverride::Delete)) {
                return Err(EditError::Deleted(id.to_string()));
            }
            entries.insert(
                id,
                ItemOverride::Replace {
                    base: fingerprint(base),
                    item,
                },
            );
        }
        None => {
            if !T::id_str(&id).starts_with(crate::ids::USER_ID_PREFIX) {
                return Err(EditError::NotUserId(id.to_string()));
            }
            entries.insert(id, ItemOverride::Custom { item });
        }
    }
    Ok(())
}

/// Deletes an item: a bundled one is marked deleted, a user one removed.
pub(crate) fn delete_item<T: FrameworkItem>(
    bundled: &ContentSet,
    overrides: &mut FrameworkOverrides,
    id: &T::Id,
) -> Result<(), crate::layers::EditError> {
    let entries = T::in_overrides_mut(overrides);
    if T::in_content(bundled).iter().any(|b| b.id() == id) {
        entries.insert(id.clone(), ItemOverride::Delete);
        Ok(())
    } else if entries.remove(id).is_some() {
        Ok(())
    } else {
        Err(crate::layers::EditError::UnknownItem(
            T::KIND.to_string(),
            id.to_string(),
        ))
    }
}

/// Restores a bundled item to its installed default.
pub(crate) fn reset_item<T: FrameworkItem>(
    bundled: &ContentSet,
    overrides: &mut FrameworkOverrides,
    id: &T::Id,
) -> Result<(), crate::layers::EditError> {
    if !T::in_content(bundled).iter().any(|b| b.id() == id) {
        return Err(crate::layers::EditError::NotBundled(id.to_string()));
    }
    T::in_overrides_mut(overrides).remove(id);
    Ok(())
}
