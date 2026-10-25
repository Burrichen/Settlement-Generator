//! The application service: everything an interface does with persistent
//! data, content and generation, in one place. The GUI's command layer and
//! the terminal interface are both thin adapters over [`AppService`], so
//! neither contains business logic of its own.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::backup::BackupKind;
use crate::bundled::bundled_content;
use crate::content::{ContentSet, ContentStatus, FaceRange, Table, TableResult, Tag};
use crate::db::Database;
use crate::dice_assist::{self, DiceAdvice, DiceFit};
use crate::editor::{self, Coverage, TagUsage};
use crate::engine::GenerationError;
use crate::explain::{self, Explanation, TablePreview};
use crate::framework::describe::{Describer, FlowContext, NodeText, PseudoLine, SettingType};
use crate::framework::edit::{self as flow_edit, FlowEdit};
use crate::framework::layers::{FrameworkItem, ItemKind};
use crate::framework::{
    Flow, ObjectType, OutlineLine, Scale, SettingDef, SettingValue, SettingValues, SharedResult,
};
use crate::ids::{FlowId, ResultId, SettingId, TableId, TagId};
use crate::layers::{ContentLayers, EditError, Origin, new_user_id};
use crate::session::{GenerationMode, GenerationSession, SessionView, ViewOptions};
use crate::settings::Settings;
use crate::settlements::{Settlement, SettlementId, SettlementSummary, SnapshotStage};
use crate::storage;
use crate::table_editor::{self, Usage};
use crate::validation::ValidationIssue;

/// Errors are plain, user-facing messages.
pub type ServiceResult<T> = Result<T, String>;

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ----- view types ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableListItem {
    pub id: TableId,
    pub name: String,
    /// `bundled`, `modified` or `custom`; absent for a deleted default.
    pub origin: Option<Origin>,
    pub built_in: bool,
    pub deleted: bool,
    pub active: bool,
    pub incomplete: bool,
    pub errors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagListItem {
    pub id: TagId,
    pub name: String,
    pub origin: Option<Origin>,
    pub built_in: bool,
    pub deleted: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentOverview {
    pub tables: Vec<TableListItem>,
    pub tags: Vec<TagListItem>,
    /// Every issue in the effective content.
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableDetail {
    pub table: Table,
    pub origin: Option<Origin>,
    pub built_in: bool,
    pub coverage: Vec<Coverage>,
    pub issues: Vec<ValidationIssue>,
    /// Everything that depends on the table: it cannot be deleted while
    /// any remain.
    #[serde(default)]
    pub usages: Vec<Usage>,
}

/// A trial roll of a table draft, with software dice rolled die by die as
/// the table's physical dice would be, and no tags active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewRoll {
    /// The roll (or why the table was skipped), then any tables it led to.
    pub events: Vec<crate::engine::GenerationEvent>,
    /// Every roll explained, line by line.
    pub traces: Vec<crate::trace::RollTrace>,
    /// What the preview assumed, in plain English.
    pub note: String,
}

/// What a complete export holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportScope {
    /// Every table, tag, setting and flow in use (defaults included).
    Everything,
    /// Only what the user made or changed.
    Customised,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagDetail {
    pub tag: Tag,
    pub origin: Option<Origin>,
    pub built_in: bool,
    pub usages: Vec<TagUsage>,
    pub issues: Vec<ValidationIssue>,
}

/// Validation of a draft before it is accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftCheck {
    pub coverage: Vec<Coverage>,
    /// Issues for the draft, plus errors it would cause elsewhere.
    pub issues: Vec<ValidationIssue>,
    /// Why the draft cannot be stored at all (e.g. a non-user ID).
    pub error: Option<String>,
}

impl DraftCheck {
    /// Whether the draft can be saved.
    pub fn is_acceptable(&self) -> bool {
        self.error.is_none()
            && !self
                .issues
                .iter()
                .any(|i| i.severity == crate::validation::Severity::Error)
    }
}

/// A tag draft in words: each effect explained, the modifiers it puts on
/// each table combined, and whether it is informational only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagInsight {
    pub effects: Vec<Explanation>,
    /// Tables the tag puts several modifiers on, with the combined preview.
    pub combined: Vec<TablePreview>,
    /// No active effects: the tag only labels the settlement (conditions
    /// may still test it).
    pub informational: bool,
}

/// A setting with what depends on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingDetail {
    pub setting: SettingDef,
    pub origin: Option<Origin>,
    /// Part of the installed content (can be restored, never removed).
    pub built_in: bool,
    pub usages: Vec<Usage>,
    pub issues: Vec<ValidationIssue>,
}

/// Validation of a flow draft, with its pseudocode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowCheck {
    /// Issues in the flow, plus errors it would cause elsewhere.
    pub issues: Vec<ValidationIssue>,
    /// The flow in plain words, read-only.
    pub pseudocode: Vec<PseudoLine>,
    /// What each step's node shows.
    pub nodes: Vec<NodeText>,
    /// Why the flow cannot be stored at all (e.g. a non-user ID).
    pub error: Option<String>,
}

impl FlowCheck {
    /// Whether the flow can be saved.
    pub fn is_acceptable(&self) -> bool {
        self.error.is_none()
            && !self
                .issues
                .iter()
                .any(|i| i.severity == crate::validation::Severity::Error)
    }
}

/// A saved settlement as the View screens show it: listing details plus the
/// stages recorded in its snapshot (never re-read from current content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSettlementView {
    #[serde(flatten)]
    pub summary: SettlementSummary,
    pub stages: Vec<SavedStage>,
}

/// A saved stage as the View screens show it: the stage as saved, plus a
/// flow stage's record laid out as lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedStage {
    #[serde(flatten)]
    pub stage: SnapshotStage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<OutlineLine>,
}

impl std::ops::Deref for SavedStage {
    type Target = SnapshotStage;
    fn deref(&self) -> &SnapshotStage {
        &self.stage
    }
}

impl From<Settlement> for SavedSettlementView {
    fn from(settlement: Settlement) -> Self {
        SavedSettlementView {
            stages: settlement
                .snapshot
                .display_stages()
                .into_iter()
                .map(|stage| SavedStage {
                    outline: stage
                        .flow
                        .as_ref()
                        .map(|f| f.outline_below())
                        .unwrap_or_default(),
                    stage,
                })
                .collect(),
            summary: settlement.summary,
        }
    }
}

/// A content-defined generation setting and the value in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentSettingView {
    pub setting: SettingDef,
    pub value: SettingValue,
}

/// One framework item (shared result, scale, setting, object type or flow)
/// in a listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkItemListing {
    pub kind: ItemKind,
    pub id: String,
    pub name: String,
    /// `bundled`, `modified` or `custom`; absent for a deleted default.
    pub origin: Option<Origin>,
    pub deleted: bool,
}

// ----- generation in progress -----------------------------------------------------

/// A settlement being generated (or a saved one being edited), with the view
/// options taken from settings when it started.
pub struct ActiveGeneration {
    pub session: GenerationSession,
    pub view: ViewOptions,
    /// The saved settlement being edited, if this is an edit.
    pub editing: Option<SettlementId>,
}

/// Result of a generation action: the session as it now stands, and the
/// error if the action could not be completed (a failed action changes
/// nothing, except that Automatic keeps the stages it resolved).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationResponse {
    pub view: SessionView,
    pub error: Option<String>,
}

impl ActiveGeneration {
    pub fn respond<T>(
        &self,
        outcome: Result<T, GenerationError>,
    ) -> ServiceResult<GenerationResponse> {
        Ok(GenerationResponse {
            view: self.session.snapshot(self.view).map_err(msg)?,
            error: outcome.err().map(msg),
        })
    }

    pub fn view(&self) -> ServiceResult<SessionView> {
        self.session.snapshot(self.view).map_err(msg)
    }
}

fn to_json(settings: &Settings) -> serde_json::Value {
    serde_json::to_value(settings).expect("settings serialise")
}

fn pre_update(to_version: &str) -> BackupKind {
    BackupKind::PreUpdate {
        from: VERSION.to_owned(),
        to: to_version.to_owned(),
    }
}

// ----- the service -------------------------------------------------------------------

pub struct AppService {
    db: Database,
}

impl AppService {
    pub fn new(db: Database) -> Self {
        AppService { db }
    }

    /// Opens the shared database in the user data directory.
    pub fn open_default() -> ServiceResult<Self> {
        storage::open_default_database().map(Self::new).map_err(msg)
    }

    // ----- updates ----------------------------------------------------------

    /// Makes user data safe before an application update to `to_version` is
    /// installed: flushes the database file and writes a verified backup
    /// beside it. Returns the backup's path. If this fails, the update must
    /// not be installed.
    pub fn prepare_for_update(&mut self, to_version: &str) -> ServiceResult<PathBuf> {
        self.db.flush().map_err(msg)?;
        self.db.backup(&pre_update(to_version)).map_err(msg)
    }

    /// Like [`Self::prepare_for_update`], for when the service could not open
    /// the database at all (for example data from a newer version, or a
    /// failed upgrade): backs up the database file directly, read-only.
    /// Returns `None` if there is no database yet.
    pub fn prepare_unopened_for_update(to_version: &str) -> ServiceResult<Option<PathBuf>> {
        let path = storage::default_database_path()
            .ok_or("The operating system did not provide a user data directory.")?;
        Database::backup_file(&path, &pre_update(to_version)).map_err(msg)
    }

    // ----- settings ---------------------------------------------------------

    pub fn settings(&self) -> ServiceResult<Settings> {
        self.db.load_settings().map_err(msg)
    }

    /// Saves and returns what is now stored. Disabled settings (presentation
    /// while Automatic) are refused.
    pub fn save_settings(&mut self, settings: &Settings) -> ServiceResult<Settings> {
        self.db.save_settings(settings).map_err(msg)?;
        self.settings()
    }

    /// Applies the user's change from `from` (what the interface was
    /// showing) to `to` onto the settings as stored now, and returns the
    /// result. Only the fields that changed are written, so a change made
    /// meanwhile in the other interface is never reverted by a stale copy.
    pub fn change_settings(&mut self, from: &Settings, to: &Settings) -> ServiceResult<Settings> {
        let (from, to) = (to_json(from), to_json(to));
        let mut stored = to_json(&self.settings()?);
        for (key, value) in to.as_object().expect("settings are an object") {
            if from.get(key) != Some(value) {
                stored[key] = value.clone();
            }
        }
        let next: Settings = serde_json::from_value(stored).map_err(msg)?;
        self.save_settings(&next)
    }

    /// The content-defined generation settings, each with the value in use
    /// (the user's choice, else the default).
    pub fn content_settings(&self) -> ServiceResult<Vec<ContentSettingView>> {
        let content = self.content()?;
        let values = self.content_setting_values()?;
        Ok(content
            .settings
            .iter()
            .map(|def| ContentSettingView {
                value: values
                    .get(&def.id)
                    .filter(|v| def.accepts(v))
                    .cloned()
                    .unwrap_or_else(|| def.default_value()),
                setting: def.clone(),
            })
            .collect())
    }

    /// Chooses a value for a content-defined setting. Values the setting
    /// cannot take are refused.
    pub fn set_content_setting(
        &mut self,
        id: &SettingId,
        value: SettingValue,
    ) -> ServiceResult<Vec<ContentSettingView>> {
        let content = self.content()?;
        let def = content
            .setting(id)
            .ok_or_else(|| format!("There is no setting `{id}`."))?;
        if !def.accepts(&value) {
            return Err(format!("{} cannot be set to that value.", def.name));
        }
        let id = def.id.clone();
        if id.as_str() == crate::framework::BARE_MINIMUM_BUILDINGS {
            // Kept in step with the application setting it replaced.
            let mut settings = self.settings()?;
            settings.bare_minimum_buildings = value == SettingValue::Toggle(true);
            self.save_settings(&settings)?;
        }
        let mut values = self.db.load_content_settings().map_err(msg)?;
        values.insert(id, value);
        self.db.save_content_settings(&values).map_err(msg)?;
        self.content_settings()
    }

    /// The on/off settings a condition can test: God Mode and the content's
    /// toggles (Bare Minimum Buildings among them).
    pub fn toggle_settings(&self) -> ServiceResult<Vec<crate::engine::Named<SettingId>>> {
        let content = self.content()?;
        let app = std::iter::once(crate::engine::Named {
            id: SettingId::new(crate::framework::GOD_MODE_SETTING),
            name: "God Mode".to_owned(),
        });
        let own = content
            .settings
            .iter()
            .filter(|s| matches!(s.kind, crate::framework::SettingKind::Toggle { .. }))
            .map(|s| crate::engine::Named {
                id: s.id.clone(),
                name: s.name.clone(),
            });
        Ok(app.chain(own).collect())
    }

    /// The stored setting values. Bare Minimum Buildings was an application
    /// setting before it became a content setting; until it is chosen again
    /// the old choice stands in.
    fn content_setting_values(&self) -> ServiceResult<SettingValues> {
        let mut values = self.db.load_content_settings().map_err(msg)?;
        let bare_minimum = SettingId::new(crate::framework::BARE_MINIMUM_BUILDINGS);
        if let std::collections::btree_map::Entry::Vacant(entry) = values.entry(bare_minimum) {
            entry.insert(SettingValue::Toggle(
                self.settings()?.bare_minimum_buildings,
            ));
        }
        Ok(values)
    }

    // ----- content ----------------------------------------------------------

    /// Bundled defaults plus the user's stored customisations.
    pub fn layers(&self) -> ServiceResult<ContentLayers> {
        let bundled = bundled_content().map_err(msg)?;
        let overrides = self.db.load_overrides().map_err(msg)?;
        Ok(ContentLayers::new(bundled, overrides))
    }

    /// Effective content. Invalid content is reported, never used.
    pub fn content(&self) -> ServiceResult<Arc<ContentSet>> {
        let merged = self.layers()?.merged();
        if merged.report.has_errors() {
            return Err(format!(
                "The generator content is invalid:\n{}",
                merged.report
            ));
        }
        Ok(Arc::new(merged.content))
    }

    /// Applies a checked content edit and stores the result. Edits that would
    /// make the content invalid are refused with the reasons.
    fn edit_content<R>(
        &mut self,
        edit: impl FnOnce(&mut ContentLayers) -> Result<R, EditError>,
    ) -> ServiceResult<(ContentLayers, R)> {
        let mut layers = self.layers()?;
        let result = layers.apply_checked(edit).map_err(msg)?;
        self.db.save_overrides(layers.overrides()).map_err(msg)?;
        Ok((layers, result))
    }

    pub fn overview(&self) -> ServiceResult<ContentOverview> {
        Ok(overview(&self.layers()?))
    }

    pub fn table_detail(&self, id: &TableId) -> ServiceResult<TableDetail> {
        table_detail(&self.layers()?, id)
    }

    pub fn tag_detail(&self, id: &TagId) -> ServiceResult<TagDetail> {
        tag_detail(&self.layers()?, id)
    }

    // ----- table editor support ----------------------------------------------

    /// The Dice Assistant's advice for results with these relative chances.
    pub fn dice_advice(weights: &[u32]) -> DiceAdvice {
        dice_assist::advise(weights)
    }

    /// Lays results with these relative chances out on `dice`.
    pub fn dice_fit(weights: &[u32], dice: &str) -> ServiceResult<DiceFit> {
        dice_assist::fit(weights, dice)
    }

    /// `table` with a Dice Assistant layout applied to `entries` (see
    /// [`table_editor::apply_fit`]).
    pub fn apply_dice_fit(
        table: &Table,
        entries: &[ResultId],
        fit: &DiceFit,
    ) -> ServiceResult<Table> {
        table_editor::apply_fit(table, entries, fit, ResultId::new(new_user_id("result")))
    }

    /// Rolls a table draft once, as generation would with no tags active,
    /// using the draft's own dice. Refused while the draft has errors.
    pub fn preview_roll(&self, table: Table) -> ServiceResult<PreviewRoll> {
        let layers = self.layers()?;
        let id = table.id.clone();
        let (trial, ()) = layers.preview(|l| l.put_table(table)).map_err(msg)?;
        let merged = trial.merged();
        let generator = merged.generator().map_err(|_| {
            "Fix the problems listed under Validation before rolling a preview.".to_owned()
        })?;
        let log = generator
            .run(&[id], &mut crate::dice::RandomFaces::from_entropy())
            .map_err(msg)?;
        Ok(PreviewRoll {
            traces: crate::trace::trace_events(&merged.content, &log.events),
            events: log.events,
            note: "A trial roll: each die is rolled separately, exactly as the table's dice would be, and nothing is saved. No tags are active, so results that need tags are rolled again.".into(),
        })
    }

    /// A new, unsaved custom table copied from `id`: same results, faces
    /// and rules, with a new ID.
    pub fn copy_table(&self, id: &TableId) -> ServiceResult<Table> {
        let mut table = self.table_detail(id)?.table;
        table.id = TableId::new(new_user_id("table"));
        table.name = format!("Copy of {}", table.name);
        Ok(table)
    }

    /// Everything in the current content that names result `result` of the
    /// draft `table`.
    pub fn result_usages(&self, table: &Table, result: &ResultId) -> ServiceResult<Vec<Usage>> {
        let content = self.layers()?.merged().content;
        Ok(table_editor::result_usages(&content, table, result))
    }

    /// Keeps an unsaved table draft, so it survives closing the editor or
    /// the app.
    pub fn save_table_draft(&mut self, table: &Table) -> ServiceResult<()> {
        let value = serde_json::to_value(table).map_err(msg)?;
        self.db
            .save_draft(&format!("table.{}", table.id), &value)
            .map_err(msg)
    }

    /// The unsaved draft of table `id`, if one was kept.
    pub fn table_draft(&self, id: &TableId) -> ServiceResult<Option<Table>> {
        Ok(self
            .db
            .load_draft(&format!("table.{id}"))
            .map_err(msg)?
            .and_then(|v| serde_json::from_value(v).ok()))
    }

    pub fn discard_table_draft(&mut self, id: &TableId) -> ServiceResult<()> {
        self.db.delete_draft(&format!("table.{id}")).map_err(msg)
    }

    /// Checks a table draft without storing it.
    pub fn check_table(&self, table: Table) -> ServiceResult<DraftCheck> {
        let layers = self.layers()?;
        let coverage = editor::coverage(&table);
        let id = table.id.clone();
        Ok(match layers.preview(|l| l.put_table(table)) {
            Ok((trial, ())) => DraftCheck {
                coverage,
                issues: editor::relevant_issues(
                    &layers.merged().report,
                    &trial.merged().report,
                    Some(&id),
                    None,
                ),
                error: None,
            },
            Err(e) => DraftCheck {
                coverage,
                issues: Vec::new(),
                error: Some(e.to_string()),
            },
        })
    }

    /// Checks a tag draft without storing it.
    pub fn check_tag(&self, tag: Tag) -> ServiceResult<DraftCheck> {
        let layers = self.layers()?;
        let id = tag.id.clone();
        Ok(match layers.preview(|l| l.put_tag(tag)) {
            Ok((trial, ())) => DraftCheck {
                coverage: Vec::new(),
                issues: editor::relevant_issues(
                    &layers.merged().report,
                    &trial.merged().report,
                    None,
                    Some(&id),
                ),
                error: None,
            },
            Err(e) => DraftCheck {
                coverage: Vec::new(),
                issues: Vec::new(),
                error: Some(e.to_string()),
            },
        })
    }

    /// Saves a table; its kept draft is then no longer needed.
    pub fn save_table(&mut self, table: Table) -> ServiceResult<TableDetail> {
        let id = table.id.clone();
        let (layers, ()) = self.edit_content(|l| l.put_table(table))?;
        self.discard_table_draft(&id)?;
        table_detail(&layers, &id)
    }

    pub fn save_tag(&mut self, tag: Tag) -> ServiceResult<TagDetail> {
        let id = tag.id.clone();
        let (layers, ()) = self.edit_content(|l| l.put_tag(tag))?;
        tag_detail(&layers, &id)
    }

    /// Deletes a table. Refused, naming what depends on it, while anything
    /// still uses it.
    pub fn delete_table(&mut self, id: &TableId) -> ServiceResult<ContentOverview> {
        let usages = self.table_detail(id)?.usages;
        if !usages.is_empty() {
            let list: Vec<&str> = usages.iter().map(|u| u.description.as_str()).collect();
            return Err(format!(
                "This table is still used by: {}. Remove those uses first.",
                list.join("; ")
            ));
        }
        let (layers, ()) = self.edit_content(|l| l.delete_table(id))?;
        self.discard_table_draft(id)?;
        Ok(overview(&layers))
    }

    /// Deletes a tag. Refused, naming what depends on it, while anything
    /// still gives, checks, adds or removes it.
    pub fn delete_tag(&mut self, id: &TagId) -> ServiceResult<ContentOverview> {
        let usages = self.tag_detail(id)?.usages;
        if !usages.is_empty() {
            let list: Vec<&str> = usages.iter().map(|u| u.description.as_str()).collect();
            return Err(format!(
                "This tag is still used: {}. Remove those uses first, or disable the tag instead.",
                list.join("; ")
            ));
        }
        let (layers, ()) = self.edit_content(|l| l.delete_tag(id))?;
        Ok(overview(&layers))
    }

    // ----- explanations -----------------------------------------------------------

    /// A tag draft's effects in plain words, with roll previews.
    pub fn explain_tag(&self, tag: &Tag) -> ServiceResult<TagInsight> {
        let content = self.content_with_tag(tag)?;
        Ok(TagInsight {
            effects: tag
                .effects
                .iter()
                .map(|def| explain::explain_effect(&content, Some(tag), def))
                .collect(),
            combined: explain::tag_previews(&content, tag),
            informational: !tag.effects.iter().any(|d| d.is_active()),
        })
    }

    /// One effect in plain words (for an effect being built).
    pub fn explain_effect(
        &self,
        tag: Option<&Tag>,
        effect: &crate::content::EffectDef,
    ) -> ServiceResult<Explanation> {
        let content = match tag {
            Some(t) => self.content_with_tag(t)?,
            None => self.layers()?.merged().content,
        };
        Ok(explain::explain_effect(&content, tag, effect))
    }

    /// A flow step's modifier in plain words, with its roll preview.
    pub fn explain_modifier(
        &self,
        table: &TableId,
        modifier: crate::content::Modifier,
    ) -> ServiceResult<Explanation> {
        let content = self.layers()?.merged().content;
        Ok(explain::explain_modifier(&content, table, modifier))
    }

    /// What `modifiers` together do to a roll of `table`.
    pub fn roll_preview(
        &self,
        table: &TableId,
        modifiers: &[crate::content::Modifier],
    ) -> ServiceResult<Vec<String>> {
        let content = self.layers()?.merged().content;
        Ok(explain::roll_preview(&content, table, modifiers))
    }

    /// The effective content with `tag` (a draft) in place.
    fn content_with_tag(&self, tag: &Tag) -> ServiceResult<ContentSet> {
        let layers = self.layers()?;
        Ok(match layers.preview(|l| l.put_tag(tag.clone())) {
            Ok((trial, ())) => trial.merged().content,
            Err(_) => layers.merged().content,
        })
    }

    // ----- settings editor --------------------------------------------------------

    /// A setting with everything that depends on it.
    pub fn setting_detail(&self, id: &SettingId) -> ServiceResult<SettingDetail> {
        let layers = self.layers()?;
        setting_detail(&layers, id)
    }

    /// A new, unsaved custom setting of `kind`.
    pub fn new_setting(kind: SettingType) -> SettingDef {
        let option = |id: &str, name: &str| crate::framework::SettingOption {
            id: id.into(),
            name: name.into(),
        };
        SettingDef {
            id: SettingId::new(new_user_id("setting")),
            name: "New setting".into(),
            description: String::new(),
            kind: match kind {
                SettingType::Toggle => crate::framework::SettingKind::Toggle { default: false },
                SettingType::Number => crate::framework::SettingKind::Number {
                    default: 1,
                    min: Some(0),
                    max: None,
                },
                SettingType::Choice => crate::framework::SettingKind::Choice {
                    options: vec![
                        option("option-1", "Option 1"),
                        option("option-2", "Option 2"),
                    ],
                    default: "option-1".into(),
                },
                SettingType::MultiSelect => crate::framework::SettingKind::MultiSelect {
                    options: vec![
                        option("option-1", "Option 1"),
                        option("option-2", "Option 2"),
                    ],
                    default: Vec::new(),
                },
                SettingType::Text => crate::framework::SettingKind::Text {
                    default: String::new(),
                },
            },
            status: Default::default(),
            note: String::new(),
        }
    }

    /// Checks a setting draft without storing it.
    pub fn check_setting(&self, setting: &SettingDef) -> ServiceResult<DraftCheck> {
        let layers = self.layers()?;
        let id = setting.id.to_string();
        Ok(match layers.preview(|l| l.put_item(setting.clone())) {
            Ok((trial, ())) => {
                let existing: std::collections::HashSet<String> = layers
                    .merged()
                    .report
                    .errors()
                    .map(|i| i.message.clone())
                    .collect();
                DraftCheck {
                    coverage: Vec::new(),
                    issues: trial
                        .merged()
                        .report
                        .issues
                        .iter()
                        .filter(|i| {
                            i.location.setting.as_deref() == Some(id.as_str())
                                || (i.severity == crate::validation::Severity::Error
                                    && !existing.contains(&i.message))
                        })
                        .cloned()
                        .collect(),
                    error: None,
                }
            }
            Err(e) => DraftCheck {
                coverage: Vec::new(),
                issues: Vec::new(),
                error: Some(e.to_string()),
            },
        })
    }

    /// Saves a setting; refused with the reasons if it would make content
    /// invalid (e.g. removing an option a condition tests).
    pub fn save_setting(&mut self, setting: SettingDef) -> ServiceResult<SettingDetail> {
        let id = setting.id.clone();
        let (layers, ()) = self.edit_content(|l| l.put_item(setting))?;
        setting_detail(&layers, &id)
    }

    /// Deletes a setting (a default is marked deleted). Refused, naming what
    /// depends on it, while anything still tests or reads it.
    pub fn delete_setting(&mut self, id: &SettingId) -> ServiceResult<Vec<FrameworkItemListing>> {
        let usages = self.setting_detail(id)?.usages;
        if !usages.is_empty() {
            let list: Vec<&str> = usages.iter().map(|u| u.description.as_str()).collect();
            return Err(format!(
                "This setting is still used: {}. Remove those uses first.",
                list.join("; ")
            ));
        }
        self.delete_framework_item(ItemKind::Setting, id.as_str())
    }

    pub fn reset_table(&mut self, id: &TableId) -> ServiceResult<ContentOverview> {
        let (layers, ()) = self.edit_content(|l| l.reset_table(id))?;
        Ok(overview(&layers))
    }

    pub fn reset_tag(&mut self, id: &TagId) -> ServiceResult<ContentOverview> {
        let (layers, ()) = self.edit_content(|l| l.reset_tag(id))?;
        Ok(overview(&layers))
    }

    /// Restores all generator content to the installed defaults.
    pub fn reset_all(&mut self) -> ServiceResult<ContentOverview> {
        let mut layers = self.layers()?;
        layers.reset_all();
        self.db.save_overrides(layers.overrides()).map_err(msg)?;
        Ok(overview(&layers))
    }

    // ----- framework items ------------------------------------------------------

    /// Every shared result, scale, setting, object type and flow, with where
    /// it came from, plus deleted defaults.
    pub fn framework_items(&self) -> ServiceResult<Vec<FrameworkItemListing>> {
        let layers = self.layers()?;
        let merged = layers.merged();
        let mut out = Vec::new();
        list_items::<SharedResult>(&merged, |r| r.name.clone(), &mut out);
        list_items::<Scale>(&merged, |s| s.name.clone(), &mut out);
        list_items::<SettingDef>(&merged, |s| s.name.clone(), &mut out);
        list_items::<ObjectType>(&merged, |o| o.name.clone(), &mut out);
        list_items::<Flow>(&merged, |f| f.name.clone(), &mut out);
        Ok(out)
    }

    /// One framework item as JSON, as it is in the effective content.
    pub fn framework_item(&self, kind: ItemKind, id: &str) -> ServiceResult<serde_json::Value> {
        let content = self.layers()?.merged().content;
        let item = match kind {
            ItemKind::SharedResult => find_item::<SharedResult>(&content, id),
            ItemKind::Scale => find_item::<Scale>(&content, id),
            ItemKind::Setting => find_item::<SettingDef>(&content, id),
            ItemKind::ObjectType => find_item::<ObjectType>(&content, id),
            ItemKind::Flow => find_item::<Flow>(&content, id),
        };
        item.ok_or_else(|| format!("There is no {kind} `{id}`."))
    }

    /// Saves a framework item given as JSON. Like table and tag edits, an
    /// item that would make the content invalid is refused with the reasons.
    pub fn save_framework_item(
        &mut self,
        kind: ItemKind,
        item: serde_json::Value,
    ) -> ServiceResult<Vec<FrameworkItemListing>> {
        fn put<T: FrameworkItem>(
            this: &mut AppService,
            item: serde_json::Value,
        ) -> ServiceResult<()> {
            let item: T = serde_json::from_value(item).map_err(msg)?;
            this.edit_content(|l| l.put_item(item))?;
            Ok(())
        }
        match kind {
            ItemKind::SharedResult => put::<SharedResult>(self, item)?,
            ItemKind::Scale => put::<Scale>(self, item)?,
            ItemKind::Setting => put::<SettingDef>(self, item)?,
            ItemKind::ObjectType => put::<ObjectType>(self, item)?,
            ItemKind::Flow => put::<Flow>(self, item)?,
        }
        self.framework_items()
    }

    /// Deletes a framework item (a default is marked deleted). Refused if
    /// something still depends on it.
    pub fn delete_framework_item(
        &mut self,
        kind: ItemKind,
        id: &str,
    ) -> ServiceResult<Vec<FrameworkItemListing>> {
        fn delete<T: FrameworkItem>(this: &mut AppService, id: &str) -> ServiceResult<()> {
            let id: T::Id = serde_json::from_value(serde_json::Value::from(id)).map_err(msg)?;
            this.edit_content(|l| l.delete_item::<T>(&id))?;
            Ok(())
        }
        match kind {
            ItemKind::SharedResult => delete::<SharedResult>(self, id)?,
            ItemKind::Scale => delete::<Scale>(self, id)?,
            ItemKind::Setting => delete::<SettingDef>(self, id)?,
            ItemKind::ObjectType => delete::<ObjectType>(self, id)?,
            ItemKind::Flow => delete::<Flow>(self, id)?,
        }
        self.framework_items()
    }

    /// Restores a bundled framework item to its installed default.
    pub fn reset_framework_item(
        &mut self,
        kind: ItemKind,
        id: &str,
    ) -> ServiceResult<Vec<FrameworkItemListing>> {
        fn reset<T: FrameworkItem>(this: &mut AppService, id: &str) -> ServiceResult<()> {
            let id: T::Id = serde_json::from_value(serde_json::Value::from(id)).map_err(msg)?;
            this.edit_content(|l| l.reset_item::<T>(&id))?;
            Ok(())
        }
        match kind {
            ItemKind::SharedResult => reset::<SharedResult>(self, id)?,
            ItemKind::Scale => reset::<Scale>(self, id)?,
            ItemKind::Setting => reset::<SettingDef>(self, id)?,
            ItemKind::ObjectType => reset::<ObjectType>(self, id)?,
            ItemKind::Flow => reset::<Flow>(self, id)?,
        }
        self.framework_items()
    }

    // ----- flow editor support -------------------------------------------------

    /// A flow as the effective content has it.
    pub fn flow(&self, id: &FlowId) -> ServiceResult<Flow> {
        self.content()?
            .flow(id)
            .cloned()
            .ok_or_else(|| format!("There is no flow `{id}`."))
    }

    /// A new, unsaved custom flow with no steps.
    pub fn new_flow() -> Flow {
        Flow {
            id: FlowId::new(new_user_id("flow")),
            name: "New flow".into(),
            description: String::new(),
            steps: Vec::new(),
            editor: Default::default(),
        }
    }

    /// An unsaved custom copy of flow `id`, steps and layout included.
    pub fn duplicate_flow(&self, id: &FlowId) -> ServiceResult<Flow> {
        let flow = self.flow(id)?;
        Ok(flow_edit::duplicate_flow(
            &flow,
            FlowId::new(new_user_id("flow")),
        ))
    }

    /// `flow` with one edit applied (nothing is stored).
    pub fn edit_flow(flow: &Flow, edit: FlowEdit) -> ServiceResult<Flow> {
        flow_edit::apply(flow, edit)
    }

    /// The effective content with `flow` (a draft) in place, for naming its
    /// steps; the stored content if the draft cannot be put in.
    fn content_with(&self, flow: &Flow) -> ServiceResult<crate::content::ContentSet> {
        let layers = self.layers()?;
        Ok(match layers.preview(|l| l.put_item(flow.clone())) {
            Ok((trial, ())) => trial.merged().content,
            Err(_) => layers.merged().content,
        })
    }

    /// What the flow editor's pickers offer while editing `flow`.
    pub fn flow_context(&self, flow: &Flow) -> ServiceResult<FlowContext> {
        let content = self.content_with(flow)?;
        Ok(Describer::new(&content, Some(flow)).context(flow))
    }

    /// Checks a flow draft without storing it, and describes it in words.
    pub fn check_flow(&self, flow: &Flow) -> ServiceResult<FlowCheck> {
        let layers = self.layers()?;
        let id = flow.id.to_string();
        Ok(match layers.preview(|l| l.put_item(flow.clone())) {
            Ok((trial, ())) => {
                let merged = trial.merged();
                let existing: std::collections::HashSet<String> = layers
                    .merged()
                    .report
                    .errors()
                    .map(|i| i.message.clone())
                    .collect();
                let issues = merged
                    .report
                    .issues
                    .iter()
                    .filter(|i| {
                        i.location.flow.as_deref() == Some(id.as_str())
                            || (i.severity == crate::validation::Severity::Error
                                && !existing.contains(&i.message))
                    })
                    .cloned()
                    .collect();
                let describer = Describer::new(&merged.content, Some(flow));
                FlowCheck {
                    issues,
                    pseudocode: describer.pseudocode(flow),
                    nodes: describer.nodes(flow),
                    error: None,
                }
            }
            Err(e) => {
                let content = layers.merged().content;
                let describer = Describer::new(&content, Some(flow));
                FlowCheck {
                    issues: Vec::new(),
                    pseudocode: describer.pseudocode(flow),
                    nodes: describer.nodes(flow),
                    error: Some(e.to_string()),
                }
            }
        })
    }

    /// A condition in words, as the condition builder shows it.
    pub fn describe_condition(
        &self,
        flow: &Flow,
        condition: &crate::content::Condition,
    ) -> ServiceResult<String> {
        let content = self.content_with(flow)?;
        Ok(Describer::new(&content, Some(flow)).condition(condition))
    }

    /// Keeps an unsaved flow draft, so it survives closing the editor.
    pub fn save_flow_draft(&mut self, flow: &Flow) -> ServiceResult<()> {
        let value = serde_json::to_value(flow).map_err(msg)?;
        self.db
            .save_draft(&format!("flow.{}", flow.id), &value)
            .map_err(msg)
    }

    /// The unsaved draft of flow `id`, if one was kept.
    pub fn flow_draft(&self, id: &FlowId) -> ServiceResult<Option<Flow>> {
        let value = self.db.load_draft(&format!("flow.{id}")).map_err(msg)?;
        // A draft that no longer reads (e.g. after an update) is dropped.
        Ok(value.and_then(|v| serde_json::from_value(v).ok()))
    }

    pub fn discard_flow_draft(&mut self, id: &FlowId) -> ServiceResult<()> {
        self.db.delete_draft(&format!("flow.{id}")).map_err(msg)
    }

    /// Saves a flow; refused, with the reasons, if it would make the content
    /// invalid. Its kept draft is then no longer needed.
    pub fn save_flow(&mut self, flow: Flow) -> ServiceResult<Flow> {
        let id = flow.id.clone();
        self.edit_content(|l| l.put_item(flow))?;
        self.discard_flow_draft(&id)?;
        self.flow(&id)
    }

    // ----- testing and simulation ---------------------------------------------------

    /// The effective content with `table` (a draft) in place, ready to
    /// generate from; refused while it has errors.
    pub fn content_with_table(&self, table: &Table) -> ServiceResult<Arc<ContentSet>> {
        let layers = self.layers()?;
        let (trial, ()) = layers
            .preview(|l| l.put_table(table.clone()))
            .map_err(msg)?;
        usable(trial)
    }

    /// The effective content with `flow` (a draft) in place, ready to
    /// generate from; refused while it has errors.
    pub fn content_with_flow(&self, flow: &Flow) -> ServiceResult<Arc<ContentSet>> {
        let layers = self.layers()?;
        let (trial, ()) = layers.preview(|l| l.put_item(flow.clone())).map_err(msg)?;
        usable(trial)
    }

    /// Rolls a table draft `runs` times (Run 100 / 1,000 / 10,000) with
    /// `tags` active, against what its dice should give.
    pub fn simulate_table(
        &self,
        table: &Table,
        runs: u32,
        tags: &[TagId],
    ) -> ServiceResult<crate::simulate::TableSimulation> {
        let content = self.content_with_table(table)?;
        crate::simulate::simulate_table(&content, &table.id, runs, tags, None)
    }

    /// Runs a flow draft `runs` times.
    pub fn simulate_flow(
        &self,
        flow: &Flow,
        runs: u32,
    ) -> ServiceResult<crate::simulate::FlowSimulation> {
        let content = self.content_with_flow(flow)?;
        crate::simulate::simulate_flow(&content, flow, runs, None)
    }

    /// Runs a flow draft once, every roll explained (Roll Once, Step
    /// Through).
    pub fn preview_flow(&self, flow: &Flow) -> ServiceResult<crate::simulate::FlowPreview> {
        let content = self.content_with_flow(flow)?;
        Ok(crate::simulate::preview_flow(&content, flow, None))
    }

    // ----- flows: dependencies and safe deletion ------------------------------------

    /// Everything that runs flow `id`.
    pub fn flow_usages(&self, id: &FlowId) -> ServiceResult<Vec<Usage>> {
        Ok(crate::dependencies::flow_usages(&*self.content()?, id))
    }

    /// Deletes a flow (a default is marked deleted). Refused, naming what
    /// depends on it, while anything still runs it.
    pub fn delete_flow(&mut self, id: &FlowId) -> ServiceResult<Vec<FrameworkItemListing>> {
        let usages = self.flow_usages(id)?;
        if !usages.is_empty() {
            let list: Vec<&str> = usages.iter().map(|u| u.description.as_str()).collect();
            return Err(format!(
                "This flow is still used: {}. Remove those uses first.",
                list.join("; ")
            ));
        }
        self.discard_flow_draft(id)?;
        self.delete_framework_item(ItemKind::Flow, id.as_str())
    }

    // ----- import and export --------------------------------------------------------

    /// `items` as a content pack; with `with_dependencies`, everything they
    /// name too (so the pack imports anywhere).
    pub fn export_items(
        &self,
        items: &[crate::transfer::PackItem],
        with_dependencies: bool,
        name: &str,
    ) -> ServiceResult<crate::transfer::ContentPack> {
        let layers = self.layers()?;
        let content = layers.merged().content;
        crate::transfer::export(
            &content,
            Some(layers.bundled()),
            items,
            with_dependencies,
            name,
        )
    }

    /// A complete content pack: everything, or only what the user made or
    /// changed (plus what that needs).
    pub fn export_everything(
        &self,
        scope: ExportScope,
    ) -> ServiceResult<crate::transfer::ContentPack> {
        let layers = self.layers()?;
        let merged = layers.merged();
        match scope {
            ExportScope::Everything => {
                Ok(crate::transfer::export_all(&merged.content, "All content"))
            }
            ExportScope::Customised => {
                use crate::transfer::{PackItem, PackKind};
                let mine = |o: &Origin| matches!(o, Origin::Modified | Origin::Custom);
                let mut items: Vec<PackItem> = Vec::new();
                for (id, o) in &merged.origins.tables {
                    if mine(o) {
                        items.push(PackItem {
                            kind: PackKind::Table,
                            id: id.to_string(),
                        });
                    }
                }
                for (id, o) in &merged.origins.tags {
                    if mine(o) {
                        items.push(PackItem {
                            kind: PackKind::Tag,
                            id: id.to_string(),
                        });
                    }
                }
                for (kind, origins) in &merged.origins.items {
                    let kind = match kind {
                        ItemKind::SharedResult => PackKind::SharedResult,
                        ItemKind::Scale => PackKind::Scale,
                        ItemKind::Setting => PackKind::Setting,
                        ItemKind::ObjectType => PackKind::ObjectType,
                        ItemKind::Flow => PackKind::Flow,
                    };
                    for (id, o) in &origins.origins {
                        if mine(o) {
                            items.push(PackItem {
                                kind,
                                id: id.clone(),
                            });
                        }
                    }
                }
                crate::transfer::export(
                    &merged.content,
                    Some(layers.bundled()),
                    &items,
                    true,
                    "My content",
                )
            }
        }
    }

    /// What importing `pack` would do, checked against the current content.
    pub fn plan_import(
        &self,
        pack: &crate::transfer::ContentPack,
        policy: crate::transfer::Collision,
    ) -> ServiceResult<crate::transfer::ImportPlan> {
        Ok(crate::transfer::plan_import(&self.layers()?, pack, policy))
    }

    /// Imports `pack`; nothing changes unless every reference checks out.
    pub fn import_pack(
        &mut self,
        pack: &crate::transfer::ContentPack,
        policy: crate::transfer::Collision,
    ) -> ServiceResult<crate::transfer::ImportPlan> {
        let mut layers = self.layers()?;
        let plan = crate::transfer::import(&mut layers, pack, policy)?;
        self.db.save_overrides(layers.overrides()).map_err(msg)?;
        Ok(plan)
    }

    /// Writes `pack` as `<name>.json` to the exports folder (or `dir`).
    pub fn write_pack(
        pack: &crate::transfer::ContentPack,
        file_name: &str,
        dir: Option<PathBuf>,
    ) -> ServiceResult<PathBuf> {
        let dir = dir
            .or_else(crate::storage::exports_dir)
            .ok_or("There is no folder to write the export to.")?;
        std::fs::create_dir_all(&dir).map_err(msg)?;
        let safe: String = file_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_.".contains(c) {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let stem = safe.trim().trim_end_matches(".json");
        let path = dir.join(format!(
            "{}.json",
            if stem.is_empty() { "content" } else { stem }
        ));
        std::fs::write(&path, serde_json::to_string_pretty(pack).map_err(msg)?).map_err(msg)?;
        Ok(path)
    }

    /// Content files in the exports folder (or `dir`), newest first.
    pub fn list_packs(dir: Option<PathBuf>) -> Vec<PathBuf> {
        let Some(dir) = dir.or_else(crate::storage::exports_dir) else {
            return Vec::new();
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .map(|e| {
                let modified = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (modified, e.path())
            })
            .collect();
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        files.into_iter().map(|(_, p)| p).collect()
    }

    /// Reads a content pack file.
    pub fn read_pack_file(path: &std::path::Path) -> ServiceResult<crate::transfer::ContentPack> {
        let text = std::fs::read_to_string(path).map_err(msg)?;
        crate::transfer::read_pack(&text)
    }

    /// A new, unsaved custom table: one result covering a d6.
    pub fn new_table() -> Table {
        Table {
            id: TableId::new(new_user_id("table")),
            name: "New table".into(),
            description: String::new(),
            dice: Some("1d6".into()),
            selection: Default::default(),
            status: Default::default(),
            status_note: String::new(),
            ordered: false,
            scale: None,
            active: true,
            eligibility: None,
            results: vec![TableResult {
                id: ResultId::new(new_user_id("result")),
                name: "New result".into(),
                shared: None,
                description: String::new(),
                range: Some(FaceRange::new(1, 6)),
                tags: Vec::new(),
                eligibility: None,
                enabled: true,
                generator: None,
                value: None,
                unique: false,
                fallback: None,
                label_variants: Vec::new(),
                then_roll: Vec::new(),
            }],
            variants: Vec::new(),
            follow_ups: Vec::new(),
        }
    }

    /// A new, unsaved custom tag.
    pub fn new_tag() -> Tag {
        Tag {
            id: TagId::new(new_user_id("tag")),
            name: "New tag".into(),
            description: String::new(),
            effects: Vec::new(),
            enabled: true,
        }
    }

    // ----- generation -------------------------------------------------------

    /// Starts a new settlement with the current settings and content.
    pub fn start_generation(&self) -> ServiceResult<ActiveGeneration> {
        let settings = self.settings()?;
        let session = GenerationSession::new(
            self.content()?,
            settings.generation_options(),
            GenerationMode::from_settings(&settings),
        )
        .map_err(msg)?
        .with_settings(self.content_setting_values()?);
        Ok(ActiveGeneration {
            session,
            view: ViewOptions::from_settings(&settings),
            editing: None,
        })
    }

    /// Reopens a saved settlement for editing in a One-table session. The
    /// snapshot is restored as-is; only the user's changes alter it.
    pub fn start_edit(&self, id: &SettlementId) -> ServiceResult<ActiveGeneration> {
        let settings = self.settings()?;
        let saved = self.db.get_settlement(id).map_err(msg)?;
        let session = GenerationSession::from_snapshot_with(
            self.content()?,
            settings.generation_options(),
            self.content_setting_values()?,
            GenerationMode::OneTable,
            &saved.snapshot,
        )
        .map_err(msg)?;
        Ok(ActiveGeneration {
            session,
            view: ViewOptions::from_settings(&settings),
            editing: Some(id.clone()),
        })
    }

    /// Saves a newly generated settlement under `name`.
    pub fn save_new(
        &mut self,
        active: &ActiveGeneration,
        name: &str,
    ) -> ServiceResult<SettlementSummary> {
        if active.editing.is_some() {
            return Err("This is an edit of a saved settlement; save the edit instead.".into());
        }
        if !active.session.is_complete() {
            return Err("Finish every stage before saving.".into());
        }
        self.db
            .create_settlement(name, active.session.to_snapshot())
            .map(|s| s.summary)
            .map_err(msg)
    }

    /// Whether an edit differs from what is saved: a new name (ignoring
    /// surrounding spaces) or any changed result. Both interfaces use this to
    /// decide whether leaving an edit needs confirming.
    pub fn edit_has_changes(&self, active: &ActiveGeneration, name: &str) -> ServiceResult<bool> {
        let id = active
            .editing
            .as_ref()
            .ok_or("No settlement is being edited.")?;
        let saved = self.db.get_settlement(id).map_err(msg)?;
        Ok(name.trim() != saved.summary.name || active.session.to_snapshot() != saved.snapshot)
    }

    /// Saves an edit as an explicit revised snapshot under `name`. Saving
    /// with nothing changed is refused, so an unchanged settlement keeps its
    /// edit time.
    pub fn save_edit(
        &mut self,
        active: &ActiveGeneration,
        name: &str,
    ) -> ServiceResult<SavedSettlementView> {
        let id = active
            .editing
            .clone()
            .ok_or("No settlement is being edited.")?;
        if !active.session.is_complete() {
            return Err("Finish every stage before saving.".into());
        }
        if !self.edit_has_changes(active, name)? {
            return Err("Nothing has changed, so there is nothing to save.".into());
        }
        self.db
            .update_settlement(&id, name, &active.session.to_snapshot())
            .map(Into::into)
            .map_err(msg)
    }

    // ----- saved settlements ------------------------------------------------

    pub fn saved_list(&self) -> ServiceResult<Vec<SettlementSummary>> {
        self.db.list_settlements().map_err(msg)
    }

    pub fn saved_get(&self, id: &SettlementId) -> ServiceResult<SavedSettlementView> {
        self.db.get_settlement(id).map(Into::into).map_err(msg)
    }

    pub fn saved_set_favourite(
        &mut self,
        id: &SettlementId,
        favourite: bool,
    ) -> ServiceResult<SavedSettlementView> {
        self.db.set_favourite(id, favourite).map_err(msg)?;
        self.saved_get(id)
    }

    /// Deletes a saved settlement. Favourites are refused by the core.
    pub fn saved_delete(&mut self, id: &SettlementId) -> ServiceResult<()> {
        self.db.delete_settlement(id).map_err(msg)
    }
}

fn overview(layers: &ContentLayers) -> ContentOverview {
    let merged = layers.merged();
    let errors_at = |id: &TableId| {
        merged
            .report
            .errors()
            .filter(|i| i.location.table.as_ref() == Some(id))
            .count()
    };
    let mut tables: Vec<TableListItem> = merged
        .content
        .tables
        .iter()
        .map(|t| TableListItem {
            id: t.id.clone(),
            name: t.name.clone(),
            origin: merged.origins.tables.get(&t.id).copied(),
            built_in: layers.bundled().table(&t.id).is_some(),
            deleted: false,
            active: t.active,
            incomplete: t.status == ContentStatus::Incomplete,
            errors: errors_at(&t.id),
        })
        .collect();
    tables.extend(merged.origins.deleted_tables.iter().filter_map(|id| {
        layers.bundled().table(id).map(|t| TableListItem {
            id: t.id.clone(),
            name: t.name.clone(),
            origin: None,
            built_in: true,
            deleted: true,
            active: false,
            incomplete: false,
            errors: 0,
        })
    }));
    let mut tags: Vec<TagListItem> = merged
        .content
        .tags
        .iter()
        .map(|t| TagListItem {
            id: t.id.clone(),
            name: t.name.clone(),
            origin: merged.origins.tags.get(&t.id).copied(),
            built_in: layers.bundled().tag(&t.id).is_some(),
            deleted: false,
            enabled: t.enabled,
        })
        .collect();
    tags.extend(merged.origins.deleted_tags.iter().filter_map(|id| {
        layers.bundled().tag(id).map(|t| TagListItem {
            id: t.id.clone(),
            name: t.name.clone(),
            origin: None,
            built_in: true,
            deleted: true,
            enabled: false,
        })
    }));
    ContentOverview {
        tables,
        tags,
        issues: merged.report.issues,
    }
}

fn table_detail(layers: &ContentLayers, id: &TableId) -> ServiceResult<TableDetail> {
    let merged = layers.merged();
    let table = merged
        .content
        .table(id)
        .cloned()
        .ok_or_else(|| format!("Table `{id}` is not part of the current content."))?;
    Ok(TableDetail {
        usages: table_editor::table_usages(&merged.content, id),
        coverage: editor::coverage(&table),
        issues: editor::relevant_issues(&merged.report, &merged.report, Some(id), None),
        origin: merged.origins.tables.get(id).copied(),
        built_in: layers.bundled().table(id).is_some(),
        table,
    })
}

fn tag_detail(layers: &ContentLayers, id: &TagId) -> ServiceResult<TagDetail> {
    let merged = layers.merged();
    let tag = merged
        .content
        .tag(id)
        .cloned()
        .ok_or_else(|| format!("Tag `{id}` is not part of the current content."))?;
    Ok(TagDetail {
        usages: editor::tag_usages(&merged.content, id),
        issues: editor::relevant_issues(&merged.report, &merged.report, None, Some(id)),
        origin: merged.origins.tags.get(id).copied(),
        built_in: layers.bundled().tag(id).is_some(),
        tag,
    })
}

fn list_items<T: FrameworkItem>(
    merged: &crate::layers::MergedContent,
    name: impl Fn(&T) -> String,
    out: &mut Vec<FrameworkItemListing>,
) {
    let origins = merged.origins.items.get(&T::KIND);
    for item in T::in_content(&merged.content) {
        let id = T::id_str(item.id()).to_owned();
        out.push(FrameworkItemListing {
            kind: T::KIND,
            origin: origins.and_then(|o| o.origins.get(&id)).copied(),
            id,
            name: name(item),
            deleted: false,
        });
    }
    for id in origins.into_iter().flat_map(|o| o.deleted.iter()) {
        out.push(FrameworkItemListing {
            kind: T::KIND,
            id: id.clone(),
            name: id.clone(),
            origin: None,
            deleted: true,
        });
    }
}

/// `trial`'s content, if generation can use it.
fn usable(trial: ContentLayers) -> ServiceResult<Arc<ContentSet>> {
    let merged = trial.merged();
    if merged.report.has_errors() {
        return Err("Fix the problems listed under Validation before testing.".into());
    }
    Ok(Arc::new(merged.content))
}

fn setting_detail(layers: &ContentLayers, id: &SettingId) -> ServiceResult<SettingDetail> {
    let merged = layers.merged();
    let setting = merged
        .content
        .setting(id)
        .cloned()
        .ok_or_else(|| format!("There is no setting `{id}`."))?;
    let key = setting.id.to_string();
    Ok(SettingDetail {
        usages: crate::dependencies::setting_usages(&merged.content, &setting.id),
        issues: merged
            .report
            .issues
            .iter()
            .filter(|i| i.location.setting.as_deref() == Some(key.as_str()))
            .cloned()
            .collect(),
        origin: merged
            .origins
            .items
            .get(&ItemKind::Setting)
            .and_then(|o| o.origins.get(&key))
            .copied(),
        built_in: layers.bundled().setting(&setting.id).is_some(),
        setting,
    })
}

fn find_item<T: FrameworkItem>(content: &ContentSet, id: &str) -> Option<serde_json::Value> {
    T::in_content(content)
        .iter()
        .find(|i| T::id_str(i.id()) == id)
        .map(|i| serde_json::to_value(i).expect("content serialises"))
}
