//! The application service: everything an interface does with persistent
//! data, content and generation, in one place. The GUI's command layer and
//! the terminal interface are both thin adapters over [`AppService`], so
//! neither contains business logic of its own.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::archive::{
    ARCHIVE_FORMAT, ARCHIVE_VERSION, ArchivedMaps, ArchivedSettlement, SettlementArchive,
};
use crate::backup::BackupKind;
use crate::bundled::bundled_content;
use crate::content::{ContentSet, ContentStatus, FaceRange, Table, TableResult, Tag};
use crate::db::Database;
use crate::dice::{FaceSource, RandomFaces};
use crate::editor::{self, Coverage, TagUsage};
use crate::engine::{GenerationError, GenerationOptions, Generator};
use crate::ids::{BuildingId, MapId, NamingSchemeId, ResultId, TableId, TagId};
use crate::layers::{ContentLayers, EditError, Origin, new_user_id};
use crate::maps::details::{self, DetailChoices};
use crate::maps::directory::{
    self, AssignOptions, DistrictNames, LocationBook, LocationEdit, LocationEntry, LocationError,
    PlaceOn, PoiEntry,
};
use crate::maps::export::{self, ExportFormat};
use crate::maps::geometry::Point;
use crate::maps::render::RenderOptions;
use crate::maps::{
    self, BuildingDetailsView, BuildingEdits, BuildingRecord, BuildingSummary, BuiltinEngine,
    DetailsError, EngineInfo, MapBuilding, MapEngineAdapter, MapGenerationOptions, MapLayout,
    MapLegendEntry, MapMarker, MapScope, MapStatus, SettlementMap, SettlementMapSet,
};
use crate::maps::{DamagedKind, DamagedRecord, SavedView};
use crate::naming::{JoinRule, SpellingException};
use crate::session::{GenerationMode, GenerationSession, SessionView, ViewOptions};
use crate::settings::Settings;
use crate::settlements::{Settlement, SettlementId, SettlementSummary, SnapshotStage};
use crate::storage;
use crate::validation::{ValidationIssue, ValidationReport};

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
    /// Naming methods that read approved names from this table's results:
    /// each result needs one name per listed variant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approved_names: Vec<editor::ApprovedNameSlots>,
    /// When this is a naming scheme's method table: each method's joining
    /// rule and spelling exceptions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub naming_methods: Vec<editor::NamingMethodDetail>,
    /// When this is a Notable Location subtype (or split) table: each
    /// result that is a canonical subtype, and where else it is listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtypes: Vec<editor::SubtypeUse>,
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

/// A saved settlement's maps as the map screens show them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementMapsView {
    pub settlement_id: SettlementId,
    pub status: MapStatus,
    /// How the settlement is mapped, when that can be told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<MapLayout>,
    /// Why maps cannot be generated now (for example a City without
    /// recorded districts). Saved maps are still shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The engine new maps would be drawn with.
    pub engine: EngineInfo,
    /// Every map the settlement gets, in order, from its saved data (for a
    /// City or Capital, its recorded districts), whether drawn yet or not.
    pub planned: Vec<PlannedMapView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps: Option<SettlementMapSet>,
    /// Numbered markers and legend of each saved map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legends: Vec<MapLegendView>,
    /// The buildings on the saved maps whose details have been generated.
    /// Every other building is ungenerated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<BuildingSummary>,
    /// The settlement's Notable Location directory: its own locations and
    /// the user's custom ones, placed or not, with their marker numbers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<LocationEntry>,
    /// Every point of interest on the saved maps, by number.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pois: Vec<PoiEntry>,
    /// Where each map was last left.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<SavedView>,
    /// Stored map data that could not be read. Everything else opens; the
    /// user can discard these with `maps_discard_damaged`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damaged: Vec<DamagedRecord>,
}

/// Structured map data, for other tools: the maps exported with everything
/// shown on them. Read-only (import a settlement export instead).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapDataExport {
    /// Always `settlement-generator.map-data`.
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: time::OffsetDateTime,
    pub settlement_id: SettlementId,
    pub settlement_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<MapLayout>,
    pub seed: u64,
    pub revision: u32,
    /// The exported maps, under the user's district names.
    pub maps: Vec<maps::SettlementMap>,
    /// Stored details of the buildings on those maps.
    pub buildings: Vec<BuildingRecord>,
    pub locations: Vec<LocationEntry>,
    pub pois: Vec<PoiEntry>,
    pub legends: Vec<MapLegendView>,
}

pub const MAP_DATA_FORMAT: &str = "settlement-generator.map-data";
pub const MAP_DATA_VERSION: u32 = 1;

/// What to export: one map (one district) or every drawn map, in which
/// formats, and where.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapExportRequest {
    /// One map; absent: every drawn map (each to its own file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<MapId>,
    pub formats: Vec<ExportFormat>,
    #[serde(default)]
    pub options: RenderOptions,
    /// The folder; absent: the default export folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
}

/// Files an export wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportResult {
    pub dir: PathBuf,
    pub files: Vec<PathBuf>,
}

/// One map a settlement gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedMapView {
    pub id: MapId,
    /// The district's name (the user's, if renamed), or the settlement's.
    pub label: String,
    /// The district's name when it is not renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_label: Option<String>,
    pub scope: MapScope,
    /// A map has been generated and saved.
    pub generated: bool,
    /// The saved map was drawn before the settlement last changed.
    pub stale: bool,
    /// What deleting this map would also delete or take off.
    pub annotations: MapAnnotations,
}

/// The user's data on one map, so deleting it can say what goes with it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapAnnotations {
    /// Buildings with generated or edited details (deleted with the map).
    pub building_details: u32,
    /// Notable Locations placed on it (taken off, and kept).
    pub locations: u32,
    /// Points of interest on it (deleted with the map).
    pub pois: u32,
}

/// The markers drawn on one map and the matching legend lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapLegendView {
    pub map_id: MapId,
    pub markers: Vec<MapMarker>,
    pub entries: Vec<MapLegendEntry>,
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

    /// Checks a naming method's joining rule and spelling exceptions
    /// without storing them.
    pub fn check_naming_method(
        &self,
        scheme: &NamingSchemeId,
        method: &ResultId,
        join: JoinRule,
        exceptions: Vec<SpellingException>,
    ) -> ServiceResult<DraftCheck> {
        let layers = self.layers()?;
        Ok(
            match layers.preview(|l| l.put_naming_method(scheme, method, join, exceptions)) {
                Ok((trial, ())) => DraftCheck {
                    coverage: Vec::new(),
                    issues: naming_issues(&layers.merged().report, &trial.merged().report, scheme),
                    error: None,
                },
                Err(e) => DraftCheck {
                    coverage: Vec::new(),
                    issues: Vec::new(),
                    error: Some(e.to_string()),
                },
            },
        )
    }

    /// Stores a naming method's joining rule and spelling exceptions, and
    /// returns its method table.
    pub fn save_naming_method(
        &mut self,
        scheme: &NamingSchemeId,
        method: &ResultId,
        join: JoinRule,
        exceptions: Vec<SpellingException>,
    ) -> ServiceResult<TableDetail> {
        let (layers, ()) =
            self.edit_content(|l| l.put_naming_method(scheme, method, join, exceptions))?;
        method_table_detail(&layers, scheme)
    }

    /// Restores a naming method's joining rule and exceptions to the
    /// default, and returns its method table.
    pub fn reset_naming_method(
        &mut self,
        scheme: &NamingSchemeId,
        method: &ResultId,
    ) -> ServiceResult<TableDetail> {
        let (layers, ()) = self.edit_content(|l| l.reset_naming_method(scheme, method))?;
        method_table_detail(&layers, scheme)
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
            ordered: false,
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
                generator: None,
                value: None,
                unique: false,
                fallback: None,
                label_variants: Vec::new(),
                approved_names: Default::default(),
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

    // ----- maps -------------------------------------------------------------

    /// A saved settlement's maps, read back as generated (never redrawn).
    /// Damaged or newer map data is an error here only; the settlement
    /// itself still opens through [`Self::saved_get`].
    ///
    /// Stored maps that cannot be read (damaged, or from a newer version) do
    /// not stop the settlement or its directory from opening: they are
    /// reported in `damaged`, with `maps_discard_damaged` (or
    /// `maps_delete`) as the way out.
    pub fn maps_get(&self, id: &SettlementId) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        match self.db.load_map_set(id) {
            Ok(set) => self.view_of(&settlement, set, &BuiltinEngine),
            Err(e) => {
                let mut view = self.view_of(&settlement, None, &BuiltinEngine)?;
                view.damaged.insert(
                    0,
                    DamagedRecord {
                        kind: DamagedKind::Maps,
                        id: String::new(),
                        problem: e.to_string(),
                    },
                );
                Ok(view)
            }
        }
    }

    /// Deletes the stored map data that could not be read (the recovery for
    /// damaged optional data). Readable data and the settlement are kept.
    pub fn maps_discard_damaged(&mut self, id: &SettlementId) -> ServiceResult<SettlementMapsView> {
        let damaged = self.maps_get(id)?.damaged;
        self.db.discard_damaged(id, &damaged).map_err(msg)?;
        self.maps_get(id)
    }

    // ----- exports and imports ---------------------------------------------

    /// Exports maps as pictures (SVG, PNG) and structured data (JSON): one
    /// map, or every drawn map, each to its own file. A City's districts
    /// are never joined into one picture. Files are written safely (whole,
    /// or not at all).
    pub fn maps_export(
        &self,
        id: &SettlementId,
        request: &MapExportRequest,
    ) -> ServiceResult<ExportResult> {
        let view = self.maps_get(id)?;
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let set = view
            .maps
            .as_ref()
            .ok_or_else(|| DetailsError::NoMaps.to_string())?;
        if request.formats.is_empty() {
            return Err("Choose at least one export format.".into());
        }
        let chosen: Vec<&maps::SettlementMap> = match &request.map {
            Some(map) => vec![
                set.map(map)
                    .ok_or_else(|| LocationError::UnknownMap(map.clone()).to_string())?,
            ],
            None => set.maps.iter().collect(),
        };
        if chosen.is_empty() {
            return Err("No map has been drawn yet.".into());
        }
        let dir = match &request.dir {
            Some(dir) => dir.clone(),
            None => storage::default_export_dir()
                .ok_or("The operating system did not provide a folder for exports.")?,
        };
        let name = &settlement.summary.name;
        let single = view.layout == Some(MapLayout::Single);
        let mut files = Vec::new();
        for map in &chosen {
            let legend = view.legends.iter().find(|l| l.map_id == map.id);
            let (markers, entries) = legend
                .map(|l| (l.markers.as_slice(), l.entries.as_slice()))
                .unwrap_or((&[], &[]));
            let title = if single {
                name.clone()
            } else {
                format!("{name} — {}", map.label)
            };
            let stem = if single {
                export::file_name(&[name])
            } else {
                export::file_name(&[name, &map.label])
            };
            let svg = maps::render::svg(map, &title, markers, entries, request.options);
            for format in &request.formats {
                let bytes = match format {
                    ExportFormat::Svg => svg.clone().into_bytes(),
                    ExportFormat::Png => export::png(&svg).map_err(msg)?,
                    ExportFormat::Json => continue,
                };
                let path = dir.join(format!("{stem}.{}", format.extension()));
                export::write_file(&path, &bytes).map_err(msg)?;
                files.push(path);
            }
        }
        if request.formats.contains(&ExportFormat::Json) {
            let ids: Vec<&MapId> = chosen.iter().map(|m| &m.id).collect();
            let on = |m: &MapId| ids.contains(&m);
            let data = MapDataExport {
                format: MAP_DATA_FORMAT.into(),
                format_version: MAP_DATA_VERSION,
                app_version: VERSION.into(),
                exported_at: crate::settlements::now(),
                settlement_id: id.clone(),
                settlement_name: name.clone(),
                layout: view.layout,
                seed: set.seed,
                revision: set.revision,
                maps: chosen.iter().map(|m| (*m).clone()).collect(),
                buildings: self
                    .db
                    .load_buildings(id)
                    .map_err(msg)?
                    .into_iter()
                    .filter(|b| on(&b.map_id))
                    .collect(),
                locations: view
                    .locations
                    .iter()
                    .filter(|l| {
                        request.map.is_none() || l.placement.as_ref().is_some_and(|p| on(&p.map_id))
                    })
                    .cloned()
                    .collect(),
                pois: view
                    .pois
                    .iter()
                    .filter(|p| on(&p.map_id))
                    .cloned()
                    .collect(),
                legends: view
                    .legends
                    .iter()
                    .filter(|l| on(&l.map_id))
                    .cloned()
                    .collect(),
            };
            let stem = match (&request.map, chosen.as_slice()) {
                (Some(_), [map]) if !single => export::file_name(&[name, &map.label]),
                _ => export::file_name(&[name]),
            };
            let path = dir.join(format!("{stem}.{}", ExportFormat::Json.extension()));
            let json = serde_json::to_string_pretty(&data).expect("map data serialises");
            export::write_file(&path, json.as_bytes()).map_err(msg)?;
            files.push(path);
        }
        Ok(ExportResult { dir, files })
    }

    /// A saved settlement with everything stored for it, as one archive.
    /// Damaged map data must be discarded first, so an export is complete.
    pub fn settlement_archive(&self, id: &SettlementId) -> ServiceResult<SettlementArchive> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let set = self.db.load_map_set(id).map_err(|e| {
            format!("The maps of this settlement could not be read ({e}). Discard them before exporting.")
        })?;
        let maps = match set {
            None => None,
            Some(set) => {
                let (buildings, damaged) = self.db.load_buildings_checked(id).map_err(msg)?;
                let book = self.db.load_location_book(id).map_err(msg)?;
                if !damaged.is_empty() || !book.damaged.is_empty() {
                    return Err(
                        "Some map data of this settlement could not be read. Discard it before exporting."
                            .into(),
                    );
                }
                Some(ArchivedMaps {
                    map_format: maps::MAP_FORMAT_VERSION,
                    set,
                    buildings,
                    locations: book.records.into_values().collect(),
                    pois: book.pois.into_values().collect(),
                    next_marker: book.next_marker,
                    next_custom: book.next_custom,
                    district_names: self.db.load_district_names(id).map_err(msg)?,
                    views: self.db.load_map_views(id).map_err(msg)?,
                })
            }
        };
        Ok(SettlementArchive {
            format: ARCHIVE_FORMAT.into(),
            format_version: ARCHIVE_VERSION,
            app_version: VERSION.into(),
            exported_at: crate::settlements::now(),
            settlement: ArchivedSettlement {
                name: settlement.summary.name,
                created_at: settlement.summary.created_at,
                app_version: settlement.summary.app_version,
                snapshot_format: crate::settlements::SNAPSHOT_FORMAT_VERSION,
                snapshot: settlement.snapshot,
            },
            maps,
        })
    }

    /// Exports a saved settlement with all its map records to one file in
    /// `dir` (or the default export folder). Returns the file written.
    pub fn settlement_export(
        &self,
        id: &SettlementId,
        dir: Option<&std::path::Path>,
    ) -> ServiceResult<PathBuf> {
        let archive = self.settlement_archive(id)?;
        let dir = match dir {
            Some(dir) => dir.to_path_buf(),
            None => storage::default_export_dir()
                .ok_or("The operating system did not provide a folder for exports.")?,
        };
        let path = dir.join(format!(
            "{}.{}",
            export::file_name(&[&archive.settlement.name]),
            crate::archive::ARCHIVE_EXTENSION
        ));
        export::write_file(&path, archive.to_json().as_bytes()).map_err(msg)?;
        Ok(path)
    }

    /// Imports a settlement export (its text) as a new saved settlement,
    /// with all its maps and map records, in one transaction.
    pub fn settlement_import_json(&mut self, text: &str) -> ServiceResult<SettlementSummary> {
        let archive = SettlementArchive::from_json(text).map_err(msg)?;
        let id = self.db.import_settlement(&archive).map_err(msg)?;
        Ok(self.db.get_settlement(&id).map_err(msg)?.summary)
    }

    /// Imports a settlement export file.
    pub fn settlement_import_file(
        &mut self,
        path: &std::path::Path,
    ) -> ServiceResult<SettlementSummary> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
        self.settlement_import_json(&text)
    }

    /// The maps view: the map set with everything stored beside it.
    fn view_of(
        &self,
        settlement: &Settlement,
        set: Option<SettlementMapSet>,
        engine: &dyn MapEngineAdapter,
    ) -> ServiceResult<SettlementMapsView> {
        let id = &settlement.summary.id;
        let (buildings, damaged) = self.db.load_buildings_checked(id).map_err(msg)?;
        let stored = Stored {
            damaged,
            buildings,
            book: self.db.load_location_book(id).map_err(msg)?,
            names: self.db.load_district_names(id).map_err(msg)?,
            views: self.db.load_map_views(id).map_err(msg)?,
        };
        Ok(maps_view(settlement, set, &stored, engine))
    }

    /// Generates (or regenerates) a saved settlement's maps with the
    /// built-in engine.
    pub fn maps_generate(
        &mut self,
        id: &SettlementId,
        options: &MapGenerationOptions,
    ) -> ServiceResult<SettlementMapsView> {
        self.maps_generate_with(&BuiltinEngine, id, options)
    }

    /// Generates a saved settlement's maps with `engine`. Every map is drawn
    /// and checked in memory first and the set is saved in one transaction,
    /// so a failure leaves the settlement and any earlier maps untouched.
    pub fn maps_generate_with(
        &mut self,
        engine: &dyn MapEngineAdapter,
        id: &SettlementId,
        options: &MapGenerationOptions,
    ) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let previous = self.db.load_map_set(id).map_err(msg)?;
        let set =
            maps::generate_map_set(engine, &settlement, previous.as_ref(), options).map_err(msg)?;
        self.db.save_map_set(&set).map_err(msg)?;
        self.view_of(&settlement, Some(set), engine)
    }

    /// Deletes a saved settlement's maps and their editable data, building
    /// details included. Works even when the stored maps cannot be read.
    pub fn maps_delete(&mut self, id: &SettlementId) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        self.db.delete_map_set(id).map_err(msg)?;
        self.view_of(&settlement, None, &BuiltinEngine)
    }

    /// Deletes one map of a City or Capital (one district's) and what is on
    /// it: its building details and points of interest. Notable Locations
    /// on it are taken off and kept. The district itself, its name and the
    /// other districts' maps are kept; the map can be drawn again (with new
    /// building IDs). Interfaces warn with the map's `annotations` first.
    pub fn maps_delete_map(
        &mut self,
        id: &SettlementId,
        map: &MapId,
    ) -> ServiceResult<SettlementMapsView> {
        let (_, set) = self.settlement_and_maps(id)?;
        if set.map(map).is_none() {
            return Err(LocationError::UnknownMap(map.clone()).to_string());
        }
        self.db.delete_map(id, map).map_err(msg)?;
        self.maps_get(id)
    }

    // ----- districts, points of interest and saved views ----------------------

    /// Names one of a City's or Capital's districts; a blank name gives back
    /// the default ("District 2"). Only a display name: the district's ID,
    /// its map and everything on it are unchanged.
    pub fn maps_district_rename(
        &mut self,
        id: &SettlementId,
        district: &str,
        name: Option<String>,
    ) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let known = maps::params::inputs_from_settlement(&settlement)
            .map(|i| i.districts.iter().any(|d| d.id == district))
            .unwrap_or(false);
        if !known {
            return Err(format!("This settlement has no district `{district}`."));
        }
        let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        if let Some(name) = &name {
            if name.contains('\n') {
                return Err("A district name must be a single line.".into());
            }
            if name.chars().count() > directory::MAX_DISTRICT_NAME_CHARS {
                return Err(format!(
                    "A district name can be at most {} characters.",
                    directory::MAX_DISTRICT_NAME_CHARS
                ));
            }
        }
        self.db
            .set_district_name(id, district, name.as_deref())
            .map_err(msg)?;
        self.maps_get(id)
    }

    /// Adds a point of interest at `position` on a map.
    pub fn maps_poi_add(
        &mut self,
        id: &SettlementId,
        map: &MapId,
        position: Point,
        label: String,
        notes: String,
    ) -> ServiceResult<SettlementMapsView> {
        let (_, set) = self.settlement_and_maps(id)?;
        self.change_locations(id, |book| {
            directory::add_poi(&set, book, map, position, label, notes)
        })?;
        self.maps_get(id)
    }

    pub fn maps_poi_update(
        &mut self,
        id: &SettlementId,
        poi: &str,
        label: String,
        notes: String,
    ) -> ServiceResult<SettlementMapsView> {
        self.change_locations(id, |book| directory::update_poi(book, poi, label, notes))?;
        self.maps_get(id)
    }

    /// Moves a point of interest within its map; its number stays.
    pub fn maps_poi_move(
        &mut self,
        id: &SettlementId,
        poi: &str,
        position: Point,
    ) -> ServiceResult<SettlementMapsView> {
        let (_, set) = self.settlement_and_maps(id)?;
        self.change_locations(id, |book| directory::move_poi(&set, book, poi, position))?;
        self.maps_get(id)
    }

    /// Deletes a point of interest; its number is not reused.
    pub fn maps_poi_delete(
        &mut self,
        id: &SettlementId,
        poi: &str,
    ) -> ServiceResult<SettlementMapsView> {
        self.change_locations(id, |book| directory::delete_poi(book, poi))?;
        self.maps_get(id)
    }

    /// Remembers where a map was left (centre, and zoom relative to its
    /// default view). Interfaces call this after the view settles, not on
    /// every movement.
    pub fn maps_save_view(&mut self, id: &SettlementId, view: &SavedView) -> ServiceResult<()> {
        let (_, set) = self.settlement_and_maps(id)?;
        let map = set
            .map(&view.map_id)
            .ok_or_else(|| LocationError::UnknownMap(view.map_id.clone()).to_string())?;
        let finite = view.centre.0.is_finite() && view.centre.1.is_finite();
        if !finite || !map.geometry.bounds.contains(view.centre) {
            return Err("The view must be centred on the map.".into());
        }
        if !(view.zoom.is_finite() && (0.01..=1000.0).contains(&view.zoom)) {
            return Err("The zoom of a saved view is out of range.".into());
        }
        self.db.save_map_view(id, view).map_err(msg)
    }

    // ----- the Notable Location directory ------------------------------------

    /// Places a Notable Location on a building of the saved maps. It is
    /// given a marker number the first time it is placed, kept for good.
    /// A location standing elsewhere is moved, and another location on this
    /// building taken off, only when `options` say so. The building's
    /// details are never touched.
    pub fn maps_location_assign(
        &mut self,
        id: &SettlementId,
        location_id: &str,
        target: &PlaceOn,
        options: AssignOptions,
    ) -> ServiceResult<SettlementMapsView> {
        let (settlement, set) = self.settlement_and_maps(id)?;
        self.change_locations(id, |book| {
            directory::assign(&settlement, &set, book, location_id, target, options)
        })?;
        self.maps_get(id)
    }

    /// Takes a Notable Location off its building. The location, its name,
    /// notes and marker number, and the building's details, are all kept.
    pub fn maps_location_unassign(
        &mut self,
        id: &SettlementId,
        location_id: &str,
    ) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        self.change_locations(id, |book| {
            directory::unassign(&settlement, book, location_id)
        })?;
        self.maps_get(id)
    }

    /// Renames a Notable Location and sets its notes (and a custom one's
    /// type). `version` is the version the change was made from.
    pub fn maps_location_update(
        &mut self,
        id: &SettlementId,
        location_id: &str,
        version: u32,
        edit: LocationEdit,
    ) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        self.change_locations(id, |book| {
            directory::update(&settlement, book, location_id, version, edit)
        })?;
        self.maps_get(id)
    }

    /// Adds a custom Notable Location, placed on a building if `place` is
    /// given (all or nothing).
    pub fn maps_location_create(
        &mut self,
        id: &SettlementId,
        edit: LocationEdit,
        place: Option<&PlaceOn>,
        options: AssignOptions,
    ) -> ServiceResult<SettlementMapsView> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let set = self.db.load_map_set(id).map_err(msg)?;
        self.change_locations(id, |book| {
            directory::create(&settlement, set.as_ref(), book, edit, place, options)
        })?;
        self.maps_get(id)
    }

    /// Deletes a custom Notable Location. Its marker number is not reused.
    pub fn maps_location_delete(
        &mut self,
        id: &SettlementId,
        location_id: &str,
    ) -> ServiceResult<SettlementMapsView> {
        self.change_locations(id, |book| directory::delete(book, location_id))?;
        self.maps_get(id)
    }

    fn change_locations<T>(
        &mut self,
        id: &SettlementId,
        change: impl FnOnce(&mut LocationBook) -> Result<T, LocationError>,
    ) -> ServiceResult<T> {
        self.db
            .update_location_book(id, change)
            .map_err(msg)?
            .map_err(msg)
    }

    // ----- building details -------------------------------------------------

    /// A building's details, for the Location Inspector. The first time a
    /// building is opened its details are generated (with software dice)
    /// and stored; every later call returns the stored details unchanged.
    /// Only this building is generated, never its neighbours.
    pub fn maps_building(
        &mut self,
        id: &SettlementId,
        map: &MapId,
        building: &BuildingId,
    ) -> ServiceResult<BuildingDetailsView> {
        self.maps_building_with(id, map, building, &mut RandomFaces::from_entropy())
    }

    /// [`Self::maps_building`] with the dice given.
    pub fn maps_building_with(
        &mut self,
        id: &SettlementId,
        map_id: &MapId,
        building_id: &BuildingId,
        faces: &mut dyn FaceSource,
    ) -> ServiceResult<BuildingDetailsView> {
        let (settlement, set) = self.settlement_and_maps(id)?;
        let (map, building) = locate(&set, map_id, building_id)?;
        let record = match self.db.load_building(id, building_id).map_err(msg)? {
            Some(record) => record,
            None => {
                let content = self.content()?;
                let generator = Generator::from_validated(&content, GenerationOptions::default());
                let generated =
                    details::generate(&generator, &content, &set.inputs.tags, map, building, faces)
                        .map_err(msg)?;
                let record = BuildingRecord::new(map.id.clone(), building.id.clone(), generated);
                self.db.insert_building(id, &record).map_err(msg)?
            }
        };
        self.building_view(&settlement, map, &record)
    }

    /// Saves the user's changes to a building's details, replacing earlier
    /// changes; a field left out of `edits` shows its generated value again.
    /// `version` is the version the changes were made from: if the details
    /// have changed since, nothing is saved. One building's row is written,
    /// nothing else.
    pub fn maps_building_edit(
        &mut self,
        id: &SettlementId,
        map: &MapId,
        building: &BuildingId,
        version: u32,
        edits: BuildingEdits,
    ) -> ServiceResult<BuildingDetailsView> {
        let edits = edits.cleaned().map_err(msg)?;
        self.change_building(id, map, building, version, |record, _, _, _| {
            Ok(record.edited(edits))
        })
    }

    /// Rolls a building's generated details again. Values the user changed
    /// are kept unless `discard_edits` (notes are always kept).
    pub fn maps_building_regenerate(
        &mut self,
        id: &SettlementId,
        map: &MapId,
        building: &BuildingId,
        version: u32,
        discard_edits: bool,
    ) -> ServiceResult<BuildingDetailsView> {
        self.maps_building_regenerate_with(
            id,
            map,
            building,
            version,
            discard_edits,
            &mut RandomFaces::from_entropy(),
        )
    }

    /// [`Self::maps_building_regenerate`] with the dice given.
    pub fn maps_building_regenerate_with(
        &mut self,
        id: &SettlementId,
        map_id: &MapId,
        building_id: &BuildingId,
        version: u32,
        discard_edits: bool,
        faces: &mut dyn FaceSource,
    ) -> ServiceResult<BuildingDetailsView> {
        let content = self.content()?;
        let generator = Generator::from_validated(&content, GenerationOptions::default());
        self.change_building(
            id,
            map_id,
            building_id,
            version,
            |record, set, map, building| {
                let generated =
                    details::generate(&generator, &content, &set.inputs.tags, map, building, faces)
                        .map_err(msg)?;
                Ok(record.regenerated(generated, discard_edits))
            },
        )
    }

    /// Gives back the generated value of every field the user changed.
    /// Notes are kept: they were never generated.
    pub fn maps_building_reset(
        &mut self,
        id: &SettlementId,
        map: &MapId,
        building: &BuildingId,
        version: u32,
    ) -> ServiceResult<BuildingDetailsView> {
        self.change_building(id, map, building, version, |record, _, _, _| {
            Ok(record.edited(record.edits.notes_only()))
        })
    }

    fn settlement_and_maps(
        &self,
        id: &SettlementId,
    ) -> ServiceResult<(Settlement, SettlementMapSet)> {
        let settlement = self.db.get_settlement(id).map_err(msg)?;
        let set = self
            .db
            .load_map_set(id)
            .map_err(msg)?
            .ok_or_else(|| DetailsError::NoMaps.to_string())?;
        // Shown, and named in messages, under the user's district names.
        let mut set = set;
        let names = self.db.load_district_names(id).map_err(msg)?;
        directory::apply_district_names(&mut set, &names);
        Ok((settlement, set))
    }

    /// Applies `change` to a generated building's stored details, if they
    /// are still at `version`, and writes that building's row only. An
    /// unchanged record is not written.
    fn change_building(
        &mut self,
        id: &SettlementId,
        map_id: &MapId,
        building_id: &BuildingId,
        version: u32,
        change: impl FnOnce(
            &BuildingRecord,
            &SettlementMapSet,
            &SettlementMap,
            &MapBuilding,
        ) -> ServiceResult<BuildingRecord>,
    ) -> ServiceResult<BuildingDetailsView> {
        let (settlement, set) = self.settlement_and_maps(id)?;
        let (map, building) = locate(&set, map_id, building_id)?;
        let record = self
            .db
            .load_building(id, building_id)
            .map_err(msg)?
            .ok_or_else(|| DetailsError::NotGenerated.to_string())?;
        if record.version != version {
            return Err(DetailsError::Changed.to_string());
        }
        let next = change(&record, &set, map, building)?;
        if next.edits == record.edits && next.generated == record.generated {
            return self.building_view(&settlement, map, &record);
        }
        if !self.db.update_building(id, &next, version).map_err(msg)? {
            return Err(DetailsError::Changed.to_string());
        }
        self.building_view(&settlement, map, &next)
    }

    fn building_view(
        &self,
        settlement: &Settlement,
        map: &SettlementMap,
        record: &BuildingRecord,
    ) -> ServiceResult<BuildingDetailsView> {
        let choices = DetailChoices::from_content(&*self.content()?);
        Ok(BuildingDetailsView::new(
            &settlement.summary.id,
            map,
            record,
            choices,
        ))
    }
}

/// The map and building named, which must be on the saved maps.
fn locate<'s>(
    set: &'s SettlementMapSet,
    map_id: &MapId,
    building_id: &BuildingId,
) -> ServiceResult<(&'s SettlementMap, &'s MapBuilding)> {
    let map = set
        .map(map_id)
        .ok_or_else(|| DetailsError::UnknownMap(map_id.clone()).to_string())?;
    let building = map.building(building_id).ok_or_else(|| {
        DetailsError::UnknownBuilding {
            map: map_id.clone(),
            building: building_id.clone(),
        }
        .to_string()
    })?;
    Ok((map, building))
}

/// Everything stored beside a map set that its view shows.
struct Stored {
    buildings: Vec<BuildingRecord>,
    /// Building rows that could not be read.
    damaged: Vec<DamagedRecord>,
    book: LocationBook,
    names: DistrictNames,
    views: Vec<SavedView>,
}

fn maps_view(
    settlement: &Settlement,
    mut set: Option<SettlementMapSet>,
    stored: &Stored,
    engine: &dyn MapEngineAdapter,
) -> SettlementMapsView {
    // Shown under the user's district names; IDs are unchanged.
    if let Some(set) = &mut set {
        directory::apply_district_names(set, &stored.names);
    }
    let locations = directory::entries(settlement, set.as_ref(), &stored.book, &stored.names);
    let pois = directory::poi_entries(set.as_ref(), &stored.book);
    // Only buildings on the saved maps: the details of buildings replaced
    // by a redraw stay stored but are not listed.
    let on_maps: Vec<&BuildingRecord> = stored
        .buildings
        .iter()
        .filter(|r| {
            set.as_ref()
                .and_then(|s| s.map(&r.map_id))
                .and_then(|m| m.building(&r.building_id))
                .is_some()
        })
        .collect();
    let annotations = |id: &MapId| MapAnnotations {
        building_details: on_maps.iter().filter(|r| &r.map_id == id).count() as u32,
        locations: locations
            .iter()
            .filter(|l| l.placement.as_ref().is_some_and(|p| &p.map_id == id))
            .count() as u32,
        pois: pois.iter().filter(|p| &p.map_id == id).count() as u32,
    };
    let buildings = on_maps.iter().map(|r| BuildingSummary::from(*r)).collect();
    let stale = set
        .as_ref()
        .map(|s| maps::stale_maps(settlement, s))
        .unwrap_or_default();
    let saved = |id: &MapId| set.as_ref().and_then(|s| s.map(id)).is_some();
    let district_label = |scope: &MapScope, label: String| match scope {
        MapScope::District { district } => {
            let named = directory::named(district, &stored.names);
            let default = (named.label != district.label).then(|| district.label.clone());
            (
                MapScope::District {
                    district: named.clone(),
                },
                named.label,
                default,
            )
        }
        MapScope::Settlement => (MapScope::Settlement, label, None),
    };
    let plan = maps::params::inputs_from_settlement(settlement)
        .and_then(|inputs| maps::params::planned_maps(&inputs));
    let (layout, problem, planned) = match plan {
        Ok((layout, plan)) => {
            let planned = plan
                .into_iter()
                .map(|p| {
                    let (scope, label, default_label) = district_label(&p.scope, p.label);
                    PlannedMapView {
                        generated: saved(&p.id),
                        stale: stale.contains(&p.id),
                        annotations: annotations(&p.id),
                        id: p.id,
                        label,
                        default_label,
                        scope,
                    }
                })
                .collect();
            (Some(layout), None, planned)
        }
        // Nothing can be generated now: list what is saved.
        Err(e) => {
            let planned = set
                .iter()
                .flat_map(|s| s.maps.iter())
                .map(|m| PlannedMapView {
                    id: m.id.clone(),
                    label: m.label.clone(),
                    default_label: None,
                    scope: m.scope.clone(),
                    generated: true,
                    stale: stale.contains(&m.id),
                    annotations: annotations(&m.id),
                })
                .collect();
            // The layout follows from the Size alone (e.g. a City saved
            // before districts existed is still mapped by district).
            let layout = set.as_ref().map(|s| s.layout).or_else(|| {
                maps::params::inputs_from_settlement(settlement)
                    .and_then(|i| maps::params::layout_for(&i))
                    .ok()
            });
            (layout, Some(e.to_string()), planned)
        }
    };
    let legends = set
        .iter()
        .flat_map(|s| s.maps.iter().map(move |m| (s, m)))
        .map(|(s, m)| {
            let (markers, entries) = maps::markers(s, &m.id, &locations, &pois);
            MapLegendView {
                map_id: m.id.clone(),
                markers,
                entries,
            }
        })
        .collect();
    let views = stored
        .views
        .iter()
        .filter(|v| saved(&v.map_id))
        .cloned()
        .collect();
    SettlementMapsView {
        settlement_id: settlement.summary.id.clone(),
        status: maps::map_status(settlement, set.as_ref()),
        layout,
        problem,
        engine: engine.info(),
        planned,
        maps: set,
        legends,
        buildings,
        locations,
        pois,
        views,
        damaged: stored
            .damaged
            .iter()
            .chain(&stored.book.damaged)
            .cloned()
            .collect(),
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
        approved_names: editor::approved_name_slots(&merged.content, id),
        naming_methods: editor::naming_methods(
            &merged.content,
            layers.bundled(),
            &merged.origins.modified_naming_methods,
            id,
        ),
        subtypes: editor::subtype_uses(&merged.content, id),
        table,
    })
}

/// The method table of a naming scheme, as the table editor shows it.
fn method_table_detail(
    layers: &ContentLayers,
    scheme: &NamingSchemeId,
) -> ServiceResult<TableDetail> {
    let table = layers
        .merged()
        .content
        .naming_scheme(scheme)
        .map(|s| s.method_table.clone())
        .ok_or_else(|| format!("Naming scheme `{scheme}` is not part of the current content."))?;
    table_detail(layers, &table)
}

/// Issues of a naming scheme in `preview`, plus errors an edit would newly
/// cause elsewhere.
fn naming_issues(
    current: &ValidationReport,
    preview: &ValidationReport,
    scheme: &NamingSchemeId,
) -> Vec<ValidationIssue> {
    let existing: std::collections::HashSet<&str> =
        current.errors().map(|i| i.message.as_str()).collect();
    preview
        .issues
        .iter()
        .filter(|i| {
            i.location.naming_scheme.as_ref() == Some(scheme)
                || (i.severity == crate::validation::Severity::Error
                    && !existing.contains(i.message.as_str()))
        })
        .cloned()
        .collect()
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
