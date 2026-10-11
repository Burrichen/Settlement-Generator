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
use crate::editor::{self, Coverage, TagUsage};
use crate::engine::GenerationError;
use crate::ids::{ResultId, TableId, TagId};
use crate::layers::{ContentLayers, EditError, Origin, new_user_id};
use crate::session::{GenerationMode, GenerationSession, SessionView, ViewOptions};
use crate::settings::Settings;
use crate::settlements::{Settlement, SettlementId, SettlementSummary, SnapshotStage};
use crate::storage;
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

/// A saved settlement as the View screens show it: listing details plus the
/// stages recorded in its snapshot (never re-read from current content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSettlementView {
    #[serde(flatten)]
    pub summary: SettlementSummary,
    pub stages: Vec<SnapshotStage>,
}

impl From<Settlement> for SavedSettlementView {
    fn from(settlement: Settlement) -> Self {
        SavedSettlementView {
            stages: settlement.snapshot.display_stages(),
            summary: settlement.summary,
        }
    }
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

    pub fn save_table(&mut self, table: Table) -> ServiceResult<TableDetail> {
        let id = table.id.clone();
        let (layers, ()) = self.edit_content(|l| l.put_table(table))?;
        table_detail(&layers, &id)
    }

    pub fn save_tag(&mut self, tag: Tag) -> ServiceResult<TagDetail> {
        let id = tag.id.clone();
        let (layers, ()) = self.edit_content(|l| l.put_tag(tag))?;
        tag_detail(&layers, &id)
    }

    pub fn delete_table(&mut self, id: &TableId) -> ServiceResult<ContentOverview> {
        let (layers, ()) = self.edit_content(|l| l.delete_table(id))?;
        Ok(overview(&layers))
    }

    pub fn delete_tag(&mut self, id: &TagId) -> ServiceResult<ContentOverview> {
        let (layers, ()) = self.edit_content(|l| l.delete_tag(id))?;
        Ok(overview(&layers))
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
            active: true,
            eligibility: None,
            results: vec![TableResult {
                id: ResultId::new(new_user_id("result")),
                name: "New result".into(),
                description: String::new(),
                range: Some(FaceRange::new(1, 6)),
                tags: Vec::new(),
                eligibility: None,
                enabled: true,
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
        .map_err(msg)?;
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
        let session = GenerationSession::from_snapshot(
            self.content()?,
            settings.generation_options(),
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
