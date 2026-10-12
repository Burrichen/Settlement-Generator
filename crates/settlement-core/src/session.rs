//! Stage-by-stage generation workflow shared by both interfaces.
//!
//! A [`GenerationSession`] walks the content's `sequence` (the pipeline of
//! stages). Each stage is resolved by rolling its table with physical dice or
//! by choosing a legal result. When an earlier stage changes, later stages
//! are re-checked in order: a stage stays resolved only if the table, variant,
//! dice, legal results and applicable follow-up rolls it was resolved under
//! are unchanged; otherwise it is invalidated (its previous result is kept
//! for display) and stops contributing tags, which may in turn invalidate
//! stages after it. This makes dependency behaviour predictable without any
//! stage-specific code.

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::VERSION;
use crate::authority::AuthorityRepresentatives;
use crate::content::{ContentSet, ContentStatus, SelectionMode, StepTarget};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, GenerationLog, GenerationOptions, Generator, Named, Request,
    ResultOption, Selections, SkipReason, StageContext, State, TableRollRecord,
};
use crate::figures::{
    AuthorityFigures, FIGURE_FIELDS, FIGURES_STAGE_ID, FigureGenerator, NamePartOptions,
    RELATIONSHIP_TABLE, apply_roles,
};
use crate::ids::{NamingSchemeId, ResultId, TableId, TagId};
use crate::naming::{
    GeneratedName, NAME_STAGE_ID, NameRecord, NameSource, generate_name, scheme_for,
};
use crate::roles::{ROLES_STAGE_ID, RoleAllocation, RoleAllocator, RoleSlotOptions};
use crate::settings::{GenerationMethod, GenerationPresentation, Settings};
use crate::settlements::{MAX_NAME_LENGTH, SettlementSnapshot, SnapshotStage};

/// How the user drives generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMode {
    /// Every stage is resolved at once.
    Automatic,
    /// One stage at a time, in order.
    Incremental,
    /// All stages shown together; any stage can be acted on.
    OneTable,
}

impl GenerationMode {
    pub fn from_settings(settings: &Settings) -> Self {
        match (settings.generation_method, settings.generation_presentation) {
            (GenerationMethod::Automatic, _) => GenerationMode::Automatic,
            (GenerationMethod::Manual, GenerationPresentation::Incremental) => {
                GenerationMode::Incremental
            }
            (GenerationMethod::Manual, GenerationPresentation::OneTable) => {
                GenerationMode::OneTable
            }
        }
    }
}

/// What a stage view includes beyond the basics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewOptions {
    /// Include emitted tags (the Tag View setting).
    pub show_tags: bool,
    /// Include full reproducibility details (a debug view).
    pub debug: bool,
}

impl ViewOptions {
    pub fn from_settings(settings: &Settings) -> Self {
        ViewOptions {
            show_tags: settings.tag_view,
            debug: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Resolved,
    Skipped,
}

/// One stage as an interface should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageView {
    pub index: usize,
    /// The pipeline stage: the table the sequence names, labelled with the
    /// stage name (e.g. "Region").
    pub stage: Named<TableId>,
    /// The table that applies now, after redirects (e.g. Kurovia Region for
    /// the Region stage).
    pub table: Named<TableId>,
    pub status: StageStatus,
    /// The Incremental cursor is on this stage.
    pub current: bool,
    /// The user may roll or choose on this stage now.
    pub actionable: bool,
    /// The table can be rolled (it has dice and is complete).
    pub can_roll: bool,
    /// The dice a roll would use now (after variants and dice effects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    /// Why the table cannot be rolled, if it is incomplete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<String>,
    /// Every result of the table, marked legal or not under the current tags
    /// (in God Mode every enabled result is legal).
    pub options: Vec<ResultOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Named<ResultId>>,
    /// The result this stage had before an earlier change invalidated it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<SkipReason>,
    /// How the result was reached: the physical roll or a choice, plus any
    /// follow-up rolls. Human-readable, not the full debug record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<StageOutcome>,
    /// Tags emitted by this stage (only with `show_tags`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Named<TagId>>>,
    /// Full reproducibility record (only with `debug`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<StageDetails>,
    /// Present on the name stage only. There, `options` are the naming
    /// methods (choosing one generates a name with it), `result` holds the
    /// name, and rolling generates a name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingView>,
    /// The stage elaborates the one before it and is shown with it; a
    /// skipped sub-stage does not apply and is left out.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sub_stage: bool,
    /// Present on the Authority Figures stage only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figures: Option<FiguresView>,
    /// Present on the Role Allocation stage only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<RolesView>,
}

/// The Role Allocation stage as an interface should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolesView {
    /// The roles, once allocated (lead first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<RoleAllocation>,
    /// The role each figure receives, in order: its allocation (single,
    /// lead or member), table and results under the current tags. Only the
    /// tables of the Authority that applies.
    pub slots: Vec<RoleSlotOptions>,
}

/// The Authority Figures stage as an interface should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiguresView {
    /// The figures and relationships, once generated. Figure tags are only
    /// included with `show_tags`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AuthorityFigures>,
    /// Each field's table and its results under the current tags, for
    /// choosing a figure's details.
    pub fields: Vec<FigureFieldOptions>,
    /// The relationship table's results, for choosing a relationship.
    pub relationship_options: Vec<ResultOption>,
    /// For each figure (in order), the parts of its generated name and the
    /// results each can take: only the name tables its species (and
    /// gender) use. Empty for a figure whose name was not generated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_parts: Vec<Vec<NamePartOptions>>,
}

/// One figure field's table and its results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureFieldOptions {
    pub field: String,
    pub label: String,
    pub table: Named<TableId>,
    pub options: Vec<ResultOption>,
}

/// The name stage as an interface should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamingView {
    /// The naming scheme of the current name, or the one that applies now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Named<NamingSchemeId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<NameSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<Named<ResultId>>,
    /// Each rolled part (only with `show_tags`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<NamePartView>,
    /// The active tags that selected the scheme, e.g. Kurovia and Ordon
    /// (only with `show_tags`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routing: Vec<Named<TagId>>,
}

/// One rolled part of a generated name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePartView {
    pub table: Named<TableId>,
    pub result: Named<ResultId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<String>,
}

/// A short account of how a stage was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageOutcome {
    /// The result was chosen rather than rolled.
    pub chosen: bool,
    /// Dice expression rolled, e.g. `1d20`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    /// The accepted roll, e.g. `14` or `3 + 5 = 8`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<String>,
    /// Earlier rolls that were rerolled (ineligible, disabled, restricted).
    pub rerolls: usize,
    pub follow_ups: Vec<FollowUpOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowUpOutcome {
    pub name: String,
    pub dice: String,
    pub roll: String,
    /// The result it changed to, if it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub becomes: Option<String>,
}

fn outcome_of(resolution: &Resolution) -> Option<StageOutcome> {
    resolution.events.iter().find_map(|e| match e {
        GenerationEvent::TableRolled(record) => Some(StageOutcome {
            chosen: record.selected,
            dice: record.dice.clone(),
            roll: record.accepted().map(|a| a.roll.to_string()),
            rerolls: record.attempts.len().saturating_sub(1),
            follow_ups: record
                .follow_ups
                .iter()
                .map(|f| FollowUpOutcome {
                    name: f.name.clone(),
                    dice: f.roll.expression.clone(),
                    roll: f.roll.to_string(),
                    becomes: f.becomes.as_ref().map(|b| b.name.clone()),
                })
                .collect(),
        }),
        _ => None,
    })
}

/// Everything needed to explain why a stage came out as it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDetails {
    /// Tags active before the stage.
    pub input_tags: Vec<Named<TagId>>,
    /// The stage's records: variant, dice expression, every die, follow-up
    /// and secondary rolls, modifiers, final result and emitted tags.
    pub events: Vec<GenerationEvent>,
}

/// The whole session as an interface should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionView {
    pub mode: GenerationMode,
    pub god_mode: bool,
    /// The Incremental cursor.
    pub cursor: usize,
    pub complete: bool,
    /// The first unresolved stage (where Automatic stopped, if it did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_pending: Option<usize>,
    pub stages: Vec<StageView>,
    /// Tags active so far (only with `show_tags`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tags: Option<Vec<Named<TagId>>>,
    /// The name stage's name, generated or typed, to offer when saving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_name: Option<String>,
}

/// What a change did to the pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageChange {
    pub stage: usize,
    /// Later stages invalidated by the change.
    pub invalidated: Vec<usize>,
    /// Later stages that were skipped but can now be resolved.
    pub reopened: Vec<usize>,
}

#[derive(Debug, Clone)]
enum StageState {
    Pending,
    Skipped(SkipReason),
    Resolved(Box<Resolution>),
}

#[derive(Debug, Clone)]
struct Resolution {
    input_tags: Vec<TagId>,
    events: Vec<GenerationEvent>,
    /// Tags this stage activated, in order.
    activated: Vec<TagId>,
    /// Context of each table record, in order (see `Generator::context`).
    /// `None` for a restored stage whose context cannot be computed with the
    /// current content: it is kept exactly as saved.
    contexts: Option<Vec<StageContext>>,
    /// The saved stage this resolution was restored from, kept verbatim
    /// until the stage is changed.
    original: Option<SnapshotStage>,
    /// The name, on the name stage.
    name: Option<NameInfo>,
    /// The figures, on the Authority Figures stage. Their rolls are kept
    /// here, not in `events`.
    figures: Option<Box<AuthorityFigures>>,
    /// The roles, on the Role Allocation stage. Their rolls are kept here,
    /// not in `events`.
    roles: Option<Box<RoleAllocation>>,
}

/// A resolved name; its rolls are the resolution's events.
#[derive(Debug, Clone)]
struct NameInfo {
    text: String,
    source: NameSource,
    scheme: Option<Named<NamingSchemeId>>,
    method: Option<Named<ResultId>>,
    app_version: String,
}

/// Result ID shown for a typed name, which has no method.
const CUSTOM_NAME_ID: &str = "custom";

/// Result ID summarising a resolved Authority Figures stage.
const FIGURES_RESULT_ID: &str = "authorityFigures";

/// Result ID summarising a resolved Role Allocation stage.
const ROLES_RESULT_ID: &str = "authorityRoles";

impl Resolution {
    fn main_result(&self) -> Option<Named<ResultId>> {
        if let Some(roles) = &self.roles {
            let titles: Vec<&str> = roles.roles.iter().map(|r| r.role.name.as_str()).collect();
            return Some(Named {
                id: ResultId::new(ROLES_RESULT_ID),
                name: titles.join(", "),
            });
        }
        if let Some(figures) = &self.figures {
            let n = figures.figures.len();
            return Some(Named {
                id: ResultId::new(FIGURES_RESULT_ID),
                name: format!("{n} Authority Figure{}", if n == 1 { "" } else { "s" }),
            });
        }
        if let Some(name) = &self.name {
            let id = name
                .method
                .as_ref()
                .map_or_else(|| ResultId::new(CUSTOM_NAME_ID), |m| m.id.clone());
            return Some(Named {
                id,
                name: name.text.clone(),
            });
        }
        self.events.iter().find_map(|e| match e {
            GenerationEvent::TableRolled(r) => Some(r.result.clone()),
            _ => None,
        })
    }

    fn records(&self) -> impl Iterator<Item = &TableRollRecord> {
        self.events.iter().filter_map(|e| match e {
            GenerationEvent::TableRolled(r) => Some(&**r),
            _ => None,
        })
    }
}

#[derive(Debug, Clone)]
enum StageKind {
    Table(TableId),
    Naming,
    AuthorityFigures,
    AuthorityRoles,
}

#[derive(Debug, Clone)]
struct Stage {
    kind: StageKind,
    /// Display name of the pipeline stage (e.g. "Region").
    label: String,
    /// Elaborates the stage before it; see `SequenceStep::sub_stage`.
    sub_stage: bool,
    state: StageState,
    previous: Option<Named<ResultId>>,
}

impl Stage {
    fn is_skipped_sub_stage(&self) -> bool {
        self.sub_stage && matches!(self.state, StageState::Skipped(_))
    }

    /// The stage's table, for table stages.
    fn table(&self) -> Option<&TableId> {
        match &self.kind {
            StageKind::Table(t) => Some(t),
            StageKind::Naming | StageKind::AuthorityFigures | StageKind::AuthorityRoles => None,
        }
    }

    fn is_naming(&self) -> bool {
        matches!(self.kind, StageKind::Naming)
    }

    fn is_figures(&self) -> bool {
        matches!(self.kind, StageKind::AuthorityFigures)
    }

    fn is_roles(&self) -> bool {
        matches!(self.kind, StageKind::AuthorityRoles)
    }

    /// The stage table recorded in snapshots.
    fn snapshot_table(&self) -> TableId {
        match &self.kind {
            StageKind::Table(t) => t.clone(),
            StageKind::Naming => TableId::new(NAME_STAGE_ID),
            StageKind::AuthorityFigures => TableId::new(FIGURES_STAGE_ID),
            StageKind::AuthorityRoles => TableId::new(ROLES_STAGE_ID),
        }
    }
}

/// A settlement being generated, stage by stage.
pub struct GenerationSession {
    content: Arc<ContentSet>,
    options: GenerationOptions,
    mode: GenerationMode,
    stages: Vec<Stage>,
    cursor: usize,
}

impl GenerationSession {
    /// Starts a session over the content's sequence. Fails with a validation
    /// report if the content is invalid.
    pub fn new(
        content: Arc<ContentSet>,
        options: GenerationOptions,
        mode: GenerationMode,
    ) -> Result<Self, GenerationError> {
        Generator::new(&content)?;
        // Every interface assumes at least one stage.
        if content.sequence.is_empty() {
            return Err(GenerationError::EmptySequence);
        }
        let stages = content
            .sequence
            .iter()
            .map(|step| {
                let (kind, fallback) = match &step.target {
                    StepTarget::Table(t) => (
                        StageKind::Table(t.clone()),
                        content
                            .table(t)
                            .map_or_else(|| t.to_string(), |t| t.name.clone()),
                    ),
                    StepTarget::Naming => (StageKind::Naming, "Name".to_owned()),
                    StepTarget::AuthorityFigures => {
                        (StageKind::AuthorityFigures, "Authority Figures".to_owned())
                    }
                    StepTarget::AuthorityRoles => (StageKind::AuthorityRoles, "Roles".to_owned()),
                };
                Stage {
                    kind,
                    label: step.name.clone().unwrap_or(fallback),
                    sub_stage: step.sub_stage,
                    state: StageState::Pending,
                    previous: None,
                }
            })
            .collect();
        Ok(GenerationSession {
            options,
            content,
            mode,
            stages,
            cursor: 0,
        })
    }

    /// Automatic generation: resolves every stage from beginning to end.
    /// `selections` supplies results for choose-only stages (e.g. the
    /// Setting Profile). Stops with a readable error if content is incomplete.
    pub fn automatic(
        content: Arc<ContentSet>,
        options: GenerationOptions,
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<Self, GenerationError> {
        let mut session = Self::new(content, options, GenerationMode::Automatic)?;
        session.run_automatic(selections, faces)?;
        Ok(session)
    }

    /// Resolves every pending stage in order with physical dice, using
    /// `selections` for stages that must be chosen. Stops at the first stage
    /// that cannot be resolved (e.g. incomplete content) and reports it; the
    /// stages before it stay resolved, so the caller can choose a result for
    /// the blocked stage (see [`next_pending`](Self::next_pending)) and run
    /// again.
    pub fn run_automatic(
        &mut self,
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        while let Some(index) = self.next_pending() {
            self.resolve(index, None, selections, faces)?;
        }
        self.cursor = self.stages.len();
        Ok(())
    }

    /// Rerolls the entire settlement from scratch. Results that cannot be
    /// rolled (choose-only stages such as Setting Profile, and incomplete
    /// tables) keep their current choice; everything else is rolled again.
    pub fn reroll_all(&mut self, faces: &mut dyn FaceSource) -> Result<(), GenerationError> {
        let mut keep = Selections::new();
        for stage in &self.stages {
            if let StageState::Resolved(res) = &stage.state {
                for event in &res.events {
                    if let GenerationEvent::TableRolled(record) = event {
                        let table = self.content.table(&record.table.id);
                        let unrollable = table.is_some_and(|t| {
                            t.selection == SelectionMode::Choose
                                || t.status == ContentStatus::Incomplete
                                || t.dice.is_none()
                        });
                        if record.selected && unrollable {
                            keep.insert(record.table.id.clone(), record.result.id.clone());
                        }
                    }
                }
            }
        }
        for stage in &mut self.stages {
            // A typed name is the user's choice, not a roll: it is kept.
            let custom = matches!(&stage.state, StageState::Resolved(res)
                if res.name.as_ref().is_some_and(|n| n.source == NameSource::Custom));
            if !custom {
                stage.state = StageState::Pending;
            }
            stage.previous = None;
        }
        self.cursor = 0;
        self.run_automatic(&keep, faces)
    }

    /// The first stage still to be resolved, if any.
    pub fn next_pending(&self) -> Option<usize> {
        self.stages
            .iter()
            .position(|s| matches!(s.state, StageState::Pending))
    }

    /// Tags active after every resolved stage, in activation order.
    pub fn active_tags(&self) -> Vec<Named<TagId>> {
        self.recorded_names(&self.tags_before(self.stages.len()))
    }

    /// Everything an interface needs to draw the session.
    pub fn snapshot(&self, view: ViewOptions) -> Result<SessionView, GenerationError> {
        Ok(SessionView {
            mode: self.mode,
            god_mode: self.options.god_mode,
            cursor: self.cursor,
            complete: self.is_complete(),
            next_pending: self.next_pending(),
            stages: self.views(view)?,
            active_tags: view.show_tags.then(|| self.active_tags()),
            suggested_name: self.name(),
        })
    }

    /// The settlement's name from the name stage, if it has one.
    pub fn name(&self) -> Option<String> {
        self.stages.iter().find_map(|s| match &s.state {
            StageState::Resolved(res) => res.name.as_ref().map(|n| n.text.clone()),
            _ => None,
        })
    }

    pub fn mode(&self) -> GenerationMode {
        self.mode
    }

    pub fn options(&self) -> GenerationOptions {
        self.options
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// The Incremental cursor (equals `stage_count()` once past the end).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_complete(&self) -> bool {
        self.stages
            .iter()
            .all(|s| !matches!(s.state, StageState::Pending))
    }

    // ----- viewing ----------------------------------------------------------

    pub fn view(&self, index: usize, view: ViewOptions) -> Result<StageView, GenerationError> {
        let stage = self.stage(index)?;
        let requested = match &stage.kind {
            StageKind::Table(t) => t,
            StageKind::Naming => return self.naming_view(index, view),
            StageKind::AuthorityFigures => return self.figures_view(index, view),
            StageKind::AuthorityRoles => return self.roles_view(index, view),
        };
        let before = self.tags_before(index);
        let state = State::with_tags(&before);
        let (table, options) = self.generator().options_for(requested, &state)?;
        let dice = self.generator().context(requested, &state, None)?.dice;
        let table_def = self.content.table(&table.id).expect("options_for found it");
        let stage_def = self.content.table(requested).expect("sequence validated");
        let mut options = options;
        if !view.show_tags {
            options.iter_mut().for_each(|o| o.tags.clear());
        }
        let incomplete = (table_def.status == ContentStatus::Incomplete).then(|| {
            if table_def.status_note.is_empty() {
                format!("{} is incomplete", table_def.name)
            } else {
                table_def.status_note.clone()
            }
        });
        let (status, result, skipped, resolution) = match &stage.state {
            StageState::Pending => (StageStatus::Pending, None, None, None),
            StageState::Skipped(reason) => (StageStatus::Skipped, None, Some(*reason), None),
            StageState::Resolved(res) => {
                (StageStatus::Resolved, res.main_result(), None, Some(res))
            }
        };
        Ok(StageView {
            index,
            stage: Named {
                id: stage_def.id.clone(),
                name: stage.label.clone(),
            },
            current: self.mode == GenerationMode::Incremental && index == self.cursor,
            actionable: self.check_access(index).is_ok(),
            dice,
            can_roll: table_def.selection == SelectionMode::Roll
                && table_def.dice.is_some()
                && incomplete.is_none(),
            incomplete,
            table,
            status,
            options,
            result,
            previous: stage.previous.clone(),
            skipped,
            outcome: resolution.and_then(|res| outcome_of(res)),
            tags: resolution
                .filter(|_| view.show_tags)
                .map(|res| self.recorded_names(&res.activated)),
            details: resolution.filter(|_| view.debug).map(|res| StageDetails {
                input_tags: self.recorded_names(&res.input_tags),
                events: res.events.clone(),
            }),
            naming: None,
            sub_stage: stage.sub_stage,
            figures: None,
            roles: None,
        })
    }

    /// The name stage: its options are the methods of the naming scheme
    /// that applies now; its result is the name.
    fn naming_view(&self, index: usize, view: ViewOptions) -> Result<StageView, GenerationError> {
        let stage = self.stage(index)?;
        let before = self.tags_before(index);
        let state = State::with_tags(&before);
        let scheme = scheme_for(&self.content, &before);
        let (table, mut options, dice, can_roll, incomplete) = match scheme {
            Some(scheme) => {
                let generator = self.generator();
                let (table, options) = generator.options_for(&scheme.method_table, &state)?;
                let dice = generator.context(&scheme.method_table, &state, None)?.dice;
                let def = self.content.table(&table.id).expect("options_for found it");
                let incomplete = (def.status == ContentStatus::Incomplete).then(|| {
                    if def.status_note.is_empty() {
                        format!("{} is incomplete", def.name)
                    } else {
                        def.status_note.clone()
                    }
                });
                let can_roll = def.selection == SelectionMode::Roll
                    && def.dice.is_some()
                    && incomplete.is_none();
                (table, options, dice, can_roll, incomplete)
            }
            None => (
                Named {
                    id: TableId::new(NAME_STAGE_ID),
                    name: stage.label.clone(),
                },
                Vec::new(),
                None,
                false,
                None,
            ),
        };
        if !view.show_tags {
            options.iter_mut().for_each(|o| o.tags.clear());
        }
        let (status, skipped, resolution) = match &stage.state {
            StageState::Pending => (StageStatus::Pending, None, None),
            StageState::Skipped(reason) => (StageStatus::Skipped, Some(*reason), None),
            StageState::Resolved(res) => (StageStatus::Resolved, None, Some(res)),
        };
        let name = resolution.and_then(|res| res.name.as_ref());
        let current_scheme = scheme.map(|s| Named {
            id: s.id.clone(),
            name: s.name.clone(),
        });
        let naming = NamingView {
            scheme: name.and_then(|n| n.scheme.clone()).or(current_scheme),
            name: name.map(|n| n.text.clone()),
            source: name.map(|n| n.source),
            method: name.and_then(|n| n.method.clone()),
            parts: match resolution {
                Some(res) if view.show_tags => res
                    .records()
                    .skip(1)
                    .filter(|r| r.routed_by.is_none())
                    .map(|r| NamePartView {
                        table: r.table.clone(),
                        result: r.result.clone(),
                        dice: r.dice.clone(),
                        roll: r.accepted().map(|a| a.roll.to_string()),
                    })
                    .collect(),
                _ => Vec::new(),
            },
            routing: match scheme {
                Some(scheme) if view.show_tags => {
                    let used: Vec<TagId> = scheme
                        .when
                        .tags()
                        .into_iter()
                        .filter(|t| before.contains(t))
                        .cloned()
                        .collect();
                    self.recorded_names(&used)
                }
                _ => Vec::new(),
            },
        };
        let generated = name.is_some_and(|n| n.source == NameSource::Generated);
        Ok(StageView {
            index,
            stage: Named {
                id: TableId::new(NAME_STAGE_ID),
                name: stage.label.clone(),
            },
            table,
            status,
            current: self.mode == GenerationMode::Incremental && index == self.cursor,
            actionable: self.check_access(index).is_ok(),
            can_roll,
            dice,
            incomplete,
            options,
            result: resolution.and_then(|res| res.main_result()),
            previous: stage.previous.clone(),
            skipped,
            outcome: resolution
                .filter(|_| generated)
                .and_then(|res| outcome_of(res)),
            tags: None,
            details: resolution.filter(|_| view.debug).map(|res| StageDetails {
                input_tags: self.recorded_names(&res.input_tags),
                events: res.events.clone(),
            }),
            naming: Some(naming),
            sub_stage: false,
            figures: None,
            roles: None,
        })
    }

    /// Every stage (the One-table presentation).
    pub fn views(&self, view: ViewOptions) -> Result<Vec<StageView>, GenerationError> {
        (0..self.stages.len()).map(|i| self.view(i, view)).collect()
    }

    // ----- acting -----------------------------------------------------------

    /// Rolls (or rerolls) a stage with physical dice.
    pub fn roll(
        &mut self,
        index: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        self.resolve(index, None, &Selections::new(), faces)
    }

    /// Chooses a result for a stage directly. `faces` is used only for the
    /// table's follow-up rolls and any secondary rolls the result triggers.
    pub fn choose(
        &mut self,
        index: usize,
        result: ResultId,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        self.resolve(index, Some(result), &Selections::new(), faces)
    }

    /// Clears a stage back to pending, invalidating dependants as needed.
    pub fn clear(&mut self, index: usize) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        let stage = &mut self.stages[index];
        if let StageState::Resolved(res) = &stage.state {
            stage.previous = res.main_result();
        }
        stage.state = StageState::Pending;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Names the settlement with a typed name on the name stage. Always
    /// possible, with or without a naming scheme. A typed name is never
    /// invalidated by changes to earlier stages, and Re-roll keeps it.
    pub fn set_custom_name(
        &mut self,
        index: usize,
        name: &str,
    ) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        if !self.stage(index)?.is_naming() {
            return Err(GenerationError::NotNamingStage { stage: index });
        }
        let text = name.trim();
        if text.is_empty() {
            return Err(GenerationError::InvalidName(
                "Enter a name for the settlement.".into(),
            ));
        }
        if text.chars().count() > MAX_NAME_LENGTH {
            return Err(GenerationError::InvalidName(format!(
                "Keep the name to {MAX_NAME_LENGTH} characters or fewer."
            )));
        }
        let before = self.tags_before(index);
        let stage = &mut self.stages[index];
        stage.state = StageState::Resolved(Box::new(Resolution {
            input_tags: before,
            events: Vec::new(),
            activated: Vec::new(),
            contexts: Some(Vec::new()),
            original: None,
            name: Some(NameInfo {
                text: text.to_owned(),
                source: NameSource::Custom,
                scheme: None,
                method: None,
                app_version: crate::VERSION.to_owned(),
            }),
            figures: None,
            roles: None,
        }));
        stage.previous = None;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Rolls every pending stage in order (choose-only stages must already
    /// be chosen). Used by One-table's "roll the rest".
    pub fn roll_remaining(
        &mut self,
        faces: &mut dyn FaceSource,
    ) -> Result<Vec<StageChange>, GenerationError> {
        let mut changes = Vec::new();
        for index in 0..self.stages.len() {
            if matches!(self.stages[index].state, StageState::Pending) {
                changes.push(self.resolve(index, None, &Selections::new(), faces)?);
            }
        }
        Ok(changes)
    }

    /// Incremental: move to the next stage once the current one is done.
    pub fn advance(&mut self) -> Result<usize, GenerationError> {
        let stage = self.stage(self.cursor)?;
        if matches!(stage.state, StageState::Pending) {
            return Err(GenerationError::StageNotResolved { stage: self.cursor });
        }
        self.cursor += 1;
        // A sub-stage that does not apply is passed over.
        while self
            .stages
            .get(self.cursor)
            .is_some_and(Stage::is_skipped_sub_stage)
        {
            self.cursor += 1;
        }
        Ok(self.cursor)
    }

    /// Incremental: move back one stage, passing over sub-stages that do not
    /// apply. Results are kept until changed.
    pub fn back(&mut self) -> usize {
        self.cursor = self.cursor.saturating_sub(1);
        while self.cursor > 0 && self.stages[self.cursor].is_skipped_sub_stage() {
            self.cursor -= 1;
        }
        self.cursor
    }

    // ----- result -----------------------------------------------------------

    /// The settlement so far, in pipeline order, ready to save as a snapshot.
    pub fn log(&self) -> GenerationLog {
        let mut events = Vec::new();
        // The name stage's rolls are kept with its name record, not here.
        for stage in &self.stages {
            let Some(requested) = stage.table() else {
                continue;
            };
            match &stage.state {
                StageState::Resolved(res) => events.extend(res.events.iter().cloned()),
                StageState::Skipped(reason) => {
                    let name = self
                        .content
                        .table(requested)
                        .map_or_else(String::new, |t| t.name.clone());
                    events.push(GenerationEvent::TableSkipped {
                        table: Named {
                            id: requested.clone(),
                            name,
                        },
                        reason: *reason,
                    });
                }
                StageState::Pending => {}
            }
        }
        GenerationLog {
            events,
            tags: self.recorded_names(&self.tags_before(self.stages.len())),
            options: self.options,
        }
    }

    /// The settlement as a saveable snapshot: the full log plus each stage's
    /// label, table, result and tags. Stages restored from a snapshot and not
    /// changed are copied verbatim.
    pub fn to_snapshot(&self) -> SettlementSnapshot {
        let stages = self
            .stages
            .iter()
            .filter_map(|stage| match &stage.state {
                // Saved before naming existed: nothing to record.
                StageState::Skipped(SkipReason::NotRecorded) => None,
                StageState::Resolved(res) => Some(res.original.clone().unwrap_or_else(|| {
                    let main = res.records().next();
                    SnapshotStage {
                        label: stage.label.clone(),
                        stage_table: stage.snapshot_table(),
                        table: main.map(|r| r.table.clone()),
                        result: res.main_result(),
                        tags: self.recorded_names(&res.activated),
                        skipped: false,
                        sub_stage: stage.sub_stage,
                        figures: res.figures.as_deref().cloned(),
                        roles: res.roles.as_deref().cloned(),
                        name: res.name.as_ref().map(|n| NameRecord {
                            name: n.text.clone(),
                            source: n.source,
                            scheme: n.scheme.clone(),
                            method: n.method.clone(),
                            rolls: res.records().cloned().collect(),
                            app_version: n.app_version.clone(),
                        }),
                    }
                })),
                StageState::Skipped(_) | StageState::Pending => Some(SnapshotStage {
                    label: stage.label.clone(),
                    stage_table: stage.snapshot_table(),
                    table: None,
                    result: None,
                    tags: Vec::new(),
                    skipped: matches!(stage.state, StageState::Skipped(_)),
                    sub_stage: stage.sub_stage,
                    figures: None,
                    roles: None,
                    name: None,
                }),
            })
            .collect();
        let log = self.log();
        SettlementSnapshot {
            authority: AuthorityRepresentatives::from_log(&log),
            log,
            stages,
        }
    }

    /// Reopens a saved snapshot for editing. Every stage keeps its saved
    /// records exactly; nothing is rerolled or re-read from current content.
    /// Dependency checks are measured against the content as it is now, so
    /// only changes the user makes can invalidate later stages.
    pub fn from_snapshot(
        content: Arc<ContentSet>,
        options: GenerationOptions,
        mode: GenerationMode,
        snapshot: &SettlementSnapshot,
    ) -> Result<Self, GenerationError> {
        let mut session = Self::new(content, options, mode)?;
        let groups = group_by_stage(&snapshot.log.events, &session.stages);
        // Saved stages are matched by their stage table, in order, so a
        // snapshot from before a stage was added (such as Name) still lines
        // up.
        let mut next_saved = 0;
        for (index, events) in groups.into_iter().enumerate() {
            let wanted = session.stages[index].snapshot_table();
            let found = snapshot.stages[next_saved.min(snapshot.stages.len())..]
                .iter()
                .position(|s| s.stage_table == wanted)
                .map(|offset| next_saved + offset);
            let original = found.map(|i| snapshot.stages[i].clone());
            if let Some(i) = found {
                next_saved = i + 1;
            }
            if let Some(original) = &original {
                session.stages[index].label = original.label.clone();
            }
            if session.stages[index].is_naming() {
                let state = session.restored_name(index, original);
                session.stages[index].state = state;
                continue;
            }
            if session.stages[index].is_figures() {
                session.stages[index].state = restored_figures(original);
                continue;
            }
            if session.stages[index].is_roles() {
                session.stages[index].state = restored_roles(original);
                continue;
            }
            let Some(events) = events else { continue };
            let state = match events.first() {
                Some(GenerationEvent::TableSkipped { reason, .. }) if events.len() == 1 => {
                    StageState::Skipped(*reason)
                }
                _ => {
                    let before = session.tags_before(index);
                    let mut activated = Vec::new();
                    for event in &events {
                        if let GenerationEvent::TableRolled(record) = event {
                            let ids: Vec<TagId> =
                                record.tags.iter().map(|t| t.id.clone()).collect();
                            for tag in session.generator().activation_closure(&ids) {
                                if !before.contains(&tag) && !activated.contains(&tag) {
                                    activated.push(tag);
                                }
                            }
                        }
                    }
                    StageState::Resolved(Box::new(Resolution {
                        contexts: session.contexts(&before, &events).ok(),
                        input_tags: before,
                        events,
                        activated,
                        original,
                        name: None,
                        figures: None,
                        roles: None,
                    }))
                }
            };
            session.stages[index].state = state;
        }
        // Stages added since the snapshot was saved stay pending, except
        // those that do not apply to it. Nothing saved is rerolled.
        for index in 0..session.stages.len() {
            session.skip_if_inapplicable(index);
        }
        session.cursor = session.stages.len();
        Ok(session)
    }

    // ----- internals --------------------------------------------------------

    /// The name stage as saved: its record restored exactly (rolls and all),
    /// skipped if no scheme applied, or "not recorded" for settlements
    /// saved before naming existed. Names are never regenerated here.
    fn restored_name(&self, index: usize, saved: Option<SnapshotStage>) -> StageState {
        let Some(saved) = saved else {
            return StageState::Skipped(SkipReason::NotRecorded);
        };
        let Some(record) = saved.name.clone() else {
            return if saved.skipped {
                StageState::Skipped(SkipReason::NoNamingScheme)
            } else {
                StageState::Pending
            };
        };
        let before = self.tags_before(index);
        let events: Vec<GenerationEvent> = record
            .rolls
            .iter()
            .map(|r| GenerationEvent::TableRolled(Box::new(r.clone())))
            .collect();
        let contexts = match record.source {
            NameSource::Custom => Some(Vec::new()),
            NameSource::Generated => self.contexts(&before, &events).ok(),
        };
        StageState::Resolved(Box::new(Resolution {
            input_tags: before,
            contexts,
            events,
            activated: Vec::new(),
            original: Some(saved),
            name: Some(NameInfo {
                text: record.name,
                source: record.source,
                scheme: record.scheme,
                method: record.method,
                app_version: record.app_version,
            }),
            figures: None,
            roles: None,
        }))
    }

    /// Generates a name on the name stage: rolls (or uses the chosen) method
    /// of the scheme that applies, rolls each of its component tables with
    /// physical dice, and joins the parts by the method's rules. Without a
    /// scheme the stage is skipped (a typed name remains possible).
    fn resolve_name(
        &mut self,
        index: usize,
        method: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        let before = self.tags_before(index);
        let Some(scheme) = scheme_for(&self.content, &before).cloned() else {
            if method.is_some() {
                return Err(GenerationError::NoNamingScheme);
            }
            let stage = &mut self.stages[index];
            stage.state = StageState::Skipped(SkipReason::NoNamingScheme);
            stage.previous = None;
            let (invalidated, reopened) = self.refresh(index + 1)?;
            return Ok(StageChange {
                stage: index,
                invalidated,
                reopened,
            });
        };
        let mut state = State::with_tags(&before);
        let GeneratedName { text, method, .. } =
            generate_name(&self.generator(), &scheme, &mut state, method, faces)?;
        let events = state.take_events();
        let resolution = Resolution {
            contexts: Some(self.contexts(&before, &events)?),
            input_tags: before,
            events,
            activated: Vec::new(),
            original: None,
            name: Some(NameInfo {
                text,
                source: NameSource::Generated,
                scheme: Some(Named {
                    id: scheme.id.clone(),
                    name: scheme.name.clone(),
                }),
                method: Some(method),
                app_version: VERSION.to_owned(),
            }),
            figures: None,
            roles: None,
        };
        let stage = &mut self.stages[index];
        stage.state = StageState::Resolved(Box::new(resolution));
        stage.previous = None;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Whether every stage before `index` is settled (resolved or skipped).
    fn settled_before(&self, index: usize) -> bool {
        self.stages[..index]
            .iter()
            .all(|s| !matches!(s.state, StageState::Pending))
    }

    /// A generator over the session's content (validated in `new`).
    fn generator(&self) -> Generator<'_> {
        Generator::from_validated(&self.content, self.options)
    }

    fn stage(&self, index: usize) -> Result<&Stage, GenerationError> {
        self.stages
            .get(index)
            .ok_or(GenerationError::NoSuchStage { stage: index })
    }

    fn check_access(&self, index: usize) -> Result<(), GenerationError> {
        self.stage(index)?;
        match self.mode {
            GenerationMode::Incremental if index != self.cursor => {
                Err(GenerationError::NotCurrentStage {
                    stage: index,
                    current: self.cursor,
                })
            }
            _ => Ok(()),
        }
    }

    /// Names for `ids` as recorded in this session's records, falling back
    /// to the current content. Keeps snapshot names stable when editing.
    fn recorded_names(&self, ids: &[TagId]) -> Vec<Named<TagId>> {
        let mut recorded: std::collections::HashMap<&TagId, &str> = Default::default();
        for stage in &self.stages {
            if let StageState::Resolved(res) = &stage.state {
                for event in &res.events {
                    if let GenerationEvent::TableRolled(r) = event {
                        for tag in &r.tags {
                            recorded.entry(&tag.id).or_insert(&tag.name);
                        }
                    }
                }
                for tag in res.original.iter().flat_map(|o| &o.tags) {
                    recorded.entry(&tag.id).or_insert(&tag.name);
                }
            }
        }
        ids.iter()
            .filter_map(|id| {
                let name = recorded
                    .get(id)
                    .map(|n| n.to_string())
                    .or_else(|| self.content.tag(id).map(|t| t.name.clone()))?;
                Some(Named {
                    id: id.clone(),
                    name,
                })
            })
            .collect()
    }

    /// Tags activated by resolved stages before `index`, in order.
    fn tags_before(&self, index: usize) -> Vec<TagId> {
        let mut tags: Vec<TagId> = Vec::new();
        for stage in &self.stages[..index] {
            if let StageState::Resolved(res) = &stage.state {
                for tag in &res.activated {
                    if !tags.contains(tag) {
                        tags.push(tag.clone());
                    }
                }
            }
        }
        tags
    }

    fn resolve(
        &mut self,
        index: usize,
        choice: Option<ResultId>,
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        let requested = match &self.stage(index)?.kind {
            StageKind::Table(t) => t.clone(),
            StageKind::Naming => return self.resolve_name(index, choice, faces),
            StageKind::AuthorityFigures => {
                if choice.is_some() {
                    return Err(GenerationError::NotFiguresStage { stage: index });
                }
                return self.resolve_figures(index, faces);
            }
            StageKind::AuthorityRoles => {
                if choice.is_some() {
                    return Err(GenerationError::NotRolesStage { stage: index });
                }
                return self.resolve_roles(index, faces);
            }
        };
        let before = self.tags_before(index);
        let mut state = State::with_tags(&before);
        let request = Request::new(requested.clone()).with_choice(choice);
        // Nothing is changed unless the stage resolves successfully.
        self.generator()
            .process(&mut state, [request].into(), selections, faces)?;
        let activated: Vec<TagId> = state.active()[before.len()..].to_vec();
        let events = state.take_events();

        let new_state = match events.first() {
            Some(GenerationEvent::TableSkipped { reason, .. }) if events.len() == 1 => {
                StageState::Skipped(*reason)
            }
            _ => StageState::Resolved(Box::new(Resolution {
                contexts: Some(self.contexts(&before, &events)?),
                input_tags: before,
                events,
                activated,
                original: None,
                name: None,
                figures: None,
                roles: None,
            })),
        };
        let stage = &mut self.stages[index];
        stage.state = new_state;
        stage.previous = None;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Recomputes the context of each table record in `events`, replaying
    /// the tags each record activates.
    fn contexts(
        &self,
        before: &[TagId],
        events: &[GenerationEvent],
    ) -> Result<Vec<StageContext>, GenerationError> {
        let mut tags = before.to_vec();
        let mut contexts = Vec::new();
        for event in events {
            if let GenerationEvent::TableRolled(record) = event {
                let requested = record.redirected_from.as_ref().unwrap_or(&record.table);
                let state = State::with_tags(&tags);
                // The result before follow-ups: what was rolled or chosen.
                let base = match (record.accepted(), record.follow_ups.first()) {
                    (Some(attempt), _) => &attempt.result.id,
                    (None, Some(follow_up)) => &follow_up.from.id,
                    (None, None) => &record.result.id,
                };
                contexts.push(
                    self.generator()
                        .context(&requested.id, &state, Some(base))?,
                );
                let ids: Vec<TagId> = record.tags.iter().map(|t| t.id.clone()).collect();
                for tag in self.generator().activation_closure(&ids) {
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        Ok(contexts)
    }

    /// Re-checks stages from `from` onwards against the current tags.
    fn refresh(&mut self, from: usize) -> Result<(Vec<usize>, Vec<usize>), GenerationError> {
        let mut invalidated = Vec::new();
        let mut reopened = Vec::new();
        for index in from..self.stages.len() {
            let before = self.tags_before(index);
            let state = State::with_tags(&before);
            if self.stages[index].is_naming() {
                match self.refresh_name(index, &before) {
                    Some(true) => invalidated.push(index),
                    Some(false) => reopened.push(index),
                    None => {}
                }
                continue;
            }
            if self.stages[index].is_roles() {
                match self.refresh_roles(index) {
                    Some(true) => invalidated.push(index),
                    Some(false) => reopened.push(index),
                    None => {}
                }
                continue;
            }
            if self.stages[index].is_figures() {
                match self.refresh_figures(index) {
                    Some(true) => invalidated.push(index),
                    Some(false) => reopened.push(index),
                    None => {}
                }
                continue;
            }
            match &self.stages[index].state {
                StageState::Resolved(res) => {
                    // A restored stage that cannot be checked against the
                    // current content is kept as saved.
                    let now = self.contexts(&before, &res.events).ok();
                    let changed = match (&now, &res.contexts) {
                        (Some(now), Some(then)) => now != then,
                        _ => false,
                    };
                    if changed {
                        let previous = res.main_result();
                        let stage = &mut self.stages[index];
                        stage.previous = previous;
                        stage.state = StageState::Pending;
                        invalidated.push(index);
                    } else if let StageState::Resolved(res) = &mut self.stages[index].state {
                        res.input_tags = before;
                        if res.contexts.is_none() {
                            res.contexts = now;
                        }
                    }
                }
                StageState::Skipped(reason) => {
                    let requested = self.stages[index].table().cloned().expect("table stage");
                    let context = self.generator().context(&requested, &state, None).ok();
                    if context.is_some_and(|c| c.skipped != Some(*reason)) {
                        self.stages[index].state = StageState::Pending;
                        reopened.push(index);
                    }
                }
                StageState::Pending => {}
            }
            if self.skip_if_inapplicable(index) {
                reopened.retain(|&i| i != index);
            }
        }
        Ok((invalidated, reopened))
    }

    /// A pending table stage that does not apply (inactive or ineligible) is
    /// skipped as soon as everything before it is settled, so nobody is
    /// asked to roll it. Whatever it held before no longer applies. Returns
    /// whether it was skipped.
    fn skip_if_inapplicable(&mut self, index: usize) -> bool {
        let stage = &self.stages[index];
        if !matches!(stage.state, StageState::Pending) || !self.settled_before(index) {
            return false;
        }
        let Some(requested) = stage.table().cloned() else {
            return false;
        };
        let state = State::with_tags(&self.tags_before(index));
        let skipped = self
            .generator()
            .context(&requested, &state, None)
            .ok()
            .and_then(|c| c.skipped);
        let Some(reason) = skipped else {
            return false;
        };
        let stage = &mut self.stages[index];
        stage.state = StageState::Skipped(reason);
        stage.previous = None;
        true
    }

    /// Re-checks the name stage. A typed name always stands. A generated
    /// name is invalidated (and kept as `previous`) when the scheme that
    /// applies, or any roll's rules, changed. Without a scheme, a pending or
    /// invalidated name stage is skipped once the stages before it are
    /// settled; a skipped one reopens when a scheme starts to apply.
    /// Returns `Some(true)` if invalidated, `Some(false)` if reopened.
    fn refresh_name(&mut self, index: usize, before: &[TagId]) -> Option<bool> {
        let scheme = scheme_for(&self.content, before).map(|s| s.id.clone());
        let settled = self.settled_before(index);
        let mut outcome = None;
        match &self.stages[index].state {
            StageState::Resolved(res) => {
                let Some(name) = &res.name else { return None };
                if name.source == NameSource::Custom {
                    return None;
                }
                let now = self.contexts(before, &res.events).ok();
                let rules_changed = match (&now, &res.contexts) {
                    (Some(now), Some(then)) => now != then,
                    _ => false,
                };
                let scheme_changed = name.scheme.as_ref().map(|s| &s.id) != scheme.as_ref();
                if rules_changed || scheme_changed {
                    let previous = res.main_result();
                    let stage = &mut self.stages[index];
                    stage.previous = previous;
                    stage.state = StageState::Pending;
                    outcome = Some(true);
                } else if let StageState::Resolved(res) = &mut self.stages[index].state {
                    res.input_tags = before.to_vec();
                    if res.contexts.is_none() {
                        res.contexts = now;
                    }
                    return None;
                }
            }
            StageState::Skipped(SkipReason::NoNamingScheme) if scheme.is_some() => {
                self.stages[index].state = StageState::Pending;
                return Some(false);
            }
            _ => {}
        }
        if scheme.is_none() && settled && matches!(self.stages[index].state, StageState::Pending) {
            self.stages[index].state = StageState::Skipped(SkipReason::NoNamingScheme);
        }
        outcome
    }
}

/// Splits a saved log into per-stage event lists. A stage starts with a
/// top-level record (not routed) for the stage's table, or a skip of it; the
/// routed records and secondary rolls that follow belong to it. Stages with
/// no events in the log are `None` (e.g. stages added since it was saved).
fn group_by_stage(
    events: &[GenerationEvent],
    stages: &[Stage],
) -> Vec<Option<Vec<GenerationEvent>>> {
    let starts = |event: &GenerationEvent, table: &TableId| match event {
        GenerationEvent::TableRolled(r) => {
            r.routed_by.is_none() && &r.redirected_from.as_ref().unwrap_or(&r.table).id == table
        }
        GenerationEvent::TableSkipped { table: t, .. } => &t.id == table,
        GenerationEvent::SecondaryRoll { .. } => false,
    };
    let mut groups: Vec<Option<Vec<GenerationEvent>>> = vec![None; stages.len()];
    let mut current: Option<usize> = None;
    for event in events {
        let next = stages
            .iter()
            .enumerate()
            .skip(current.map_or(0, |c| c + 1))
            .find(|(_, s)| s.table().is_some_and(|t| starts(event, t)))
            .map(|(i, _)| i);
        match next {
            Some(index) => {
                groups[index] = Some(vec![event.clone()]);
                current = Some(index);
            }
            None => {
                if let Some(group) = current.and_then(|c| groups[c].as_mut()) {
                    group.push(event.clone());
                }
            }
        }
    }
    groups
}

// ----- Authority Figures ----------------------------------------------------------

impl GenerationSession {
    /// Rolls a field of one Authority Figure again, keeping the rest. A new
    /// species or gender routes the figure's name afresh.
    pub fn roll_figure_field(
        &mut self,
        index: usize,
        figure: usize,
        field: &str,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            g.set_field(f, figure, field, None, faces)
        })
    }

    /// Chooses a field's result for one Authority Figure (legal results
    /// only, as on any stage; God Mode lifts eligibility).
    pub fn choose_figure_field(
        &mut self,
        index: usize,
        figure: usize,
        field: &str,
        result: ResultId,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            g.set_field(f, figure, field, Some(result), faces)
        })
    }

    /// Generates one figure's name again with the naming scheme its species
    /// and gender select. Fails clearly when none applies.
    pub fn roll_figure_name(
        &mut self,
        index: usize,
        figure: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            g.rename(f, figure, None, faces)?;
            if f.name.text().is_none() {
                return Err(GenerationError::NoFigureNamingScheme);
            }
            Ok(())
        })
    }

    /// Rolls one part of a figure's generated name again (e.g. the given
    /// name), keeping the rest.
    pub fn roll_figure_name_part(
        &mut self,
        index: usize,
        figure: usize,
        part: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            g.set_name_part(f, figure, part, None, faces)
        })
    }

    /// Chooses one part of a figure's generated name from its species'
    /// own name table.
    pub fn choose_figure_name_part(
        &mut self,
        index: usize,
        figure: usize,
        part: usize,
        result: ResultId,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            g.set_name_part(f, figure, part, Some(result), faces)
        })
    }

    /// Names one figure with a typed name: always possible, with or without
    /// naming data.
    pub fn set_figure_name(
        &mut self,
        index: usize,
        figure: usize,
        name: &str,
    ) -> Result<StageChange, GenerationError> {
        let text = name.trim();
        if text.is_empty() {
            return Err(GenerationError::InvalidName("Enter a name.".into()));
        }
        if text.chars().count() > MAX_NAME_LENGTH {
            return Err(GenerationError::InvalidName(format!(
                "Keep the name to {MAX_NAME_LENGTH} characters or fewer."
            )));
        }
        self.change_figures(index, |_, record| {
            let f = record
                .figures
                .get_mut(figure)
                .ok_or(GenerationError::NoSuchFigure { index: figure })?;
            crate::figures::set_custom_name(f, figure, text.to_owned());
            Ok(())
        })
    }

    /// Rolls one relationship again (by position: 1–2, 1–3, …).
    pub fn roll_relationship(
        &mut self,
        index: usize,
        relationship: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.set_relationship(index, relationship, None, faces)
    }

    /// Chooses one relationship's result.
    pub fn choose_relationship(
        &mut self,
        index: usize,
        relationship: usize,
        result: ResultId,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.set_relationship(index, relationship, Some(result), faces)
    }

    fn set_relationship(
        &mut self,
        index: usize,
        relationship: usize,
        choice: Option<ResultId>,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_figures(index, |g, record| {
            let r = record.relationships.get_mut(relationship).ok_or(
                GenerationError::NoSuchRelationship {
                    index: relationship,
                },
            )?;
            r.roll = g.relationship(choice, faces)?;
            Ok(())
        })
    }

    /// Applies `change` to a copy of the stage's figures and keeps it only
    /// if it succeeds.
    fn change_figures(
        &mut self,
        index: usize,
        change: impl FnOnce(
            &FigureGenerator<'_, '_>,
            &mut AuthorityFigures,
        ) -> Result<(), GenerationError>,
    ) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        let stage = self.stage(index)?;
        if !stage.is_figures() {
            return Err(GenerationError::NotFiguresStage { stage: index });
        }
        let StageState::Resolved(res) = &stage.state else {
            return Err(GenerationError::FiguresNotGenerated);
        };
        let mut record = res
            .figures
            .as_deref()
            .cloned()
            .ok_or(GenerationError::FiguresNotGenerated)?;
        let before = self.tags_before(index);
        let generator = self.generator();
        change(
            &FigureGenerator {
                generator: &generator,
                content: &self.content,
                settlement_tags: &before,
            },
            &mut record,
        )?;
        if let StageState::Resolved(res) = &mut self.stages[index].state {
            res.figures = Some(Box::new(record));
            // Changed: the snapshot is written afresh from now on.
            res.original = None;
        }
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Generates every figure and relationship of the Authority Figures
    /// stage. Skipped when there is no Authority to represent.
    fn resolve_figures(
        &mut self,
        index: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        let before = self.tags_before(index);
        let state = match self.representatives_before(index) {
            Some(request) => {
                // Roles are allocated first, by the Role Allocation stage.
                let roles = match self.roles_before(index) {
                    RolesBefore::Allocated(roles) if roles.request == request => roles.roles,
                    RolesBefore::Allocated(_) => return Err(GenerationError::RolesDoNotMatch),
                    RolesBefore::Pending => return Err(GenerationError::RolesNotAllocated),
                    RolesBefore::None => Vec::new(),
                };
                let generator = self.generator();
                let record = FigureGenerator {
                    generator: &generator,
                    content: &self.content,
                    settlement_tags: &before,
                }
                .generate(&request, &roles, faces)?;
                StageState::Resolved(Box::new(Resolution {
                    input_tags: before,
                    events: Vec::new(),
                    activated: Vec::new(),
                    contexts: Some(Vec::new()),
                    original: None,
                    name: None,
                    figures: Some(Box::new(record)),
                    roles: None,
                }))
            }
            None if self.settled_before(index) => StageState::Skipped(SkipReason::NotEligible),
            None => return Err(GenerationError::FiguresNeedAuthority),
        };
        let stage = &mut self.stages[index];
        stage.state = state;
        stage.previous = None;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// The Authority to represent, from the stages before `index`.
    fn representatives_before(&self, index: usize) -> Option<AuthorityRepresentatives> {
        let mut events = Vec::new();
        for stage in &self.stages[..index] {
            if let StageState::Resolved(res) = &stage.state {
                events.extend(res.events.iter().cloned());
            }
        }
        let log = GenerationLog {
            events,
            tags: self.recorded_names(&self.tags_before(index)),
            options: self.options,
        };
        AuthorityRepresentatives::from_log(&log)
    }

    /// Re-checks the Authority Figures stage. Generated figures stand while
    /// the Authority they represent is unchanged; otherwise they are
    /// invalidated (kept as `previous`). With no Authority to represent, a
    /// pending stage is skipped once the stages before it are settled, and
    /// a skipped one reopens when there is one again. Returns `Some(true)`
    /// if invalidated, `Some(false)` if reopened.
    fn refresh_figures(&mut self, index: usize) -> Option<bool> {
        let request = self.representatives_before(index);
        let settled = self.settled_before(index);
        let mut outcome = None;
        let roles = self.roles_before(index);
        match &self.stages[index].state {
            StageState::Resolved(res) => {
                let mut same = res
                    .figures
                    .as_ref()
                    .is_some_and(|f| Some(&f.request) == request.as_ref());
                // The same Authority with different roles: the figures stand
                // and take their new roles and titles.
                let mut retitled = None;
                if let (true, Some(figures), RolesBefore::Allocated(roles)) =
                    (same, res.figures.as_deref(), &roles)
                {
                    let mut figures = figures.clone();
                    match apply_roles(&mut figures, &roles.roles) {
                        Ok(()) if Some(&figures) != res.figures.as_deref() => {
                            retitled = Some(figures);
                        }
                        Ok(()) => {}
                        Err(_) => same = false,
                    }
                }
                if let Some(figures) = retitled {
                    if let StageState::Resolved(res) = &mut self.stages[index].state {
                        res.figures = Some(Box::new(figures));
                        res.original = None;
                    }
                } else if !same {
                    let previous = res.main_result();
                    let stage = &mut self.stages[index];
                    stage.previous = previous;
                    stage.state = StageState::Pending;
                    outcome = Some(true);
                }
            }
            StageState::Skipped(SkipReason::NotEligible) if request.is_some() => {
                self.stages[index].state = StageState::Pending;
                return Some(false);
            }
            _ => {}
        }
        if request.is_none() && settled && matches!(self.stages[index].state, StageState::Pending) {
            let stage = &mut self.stages[index];
            stage.state = StageState::Skipped(SkipReason::NotEligible);
            stage.previous = None;
        }
        outcome
    }

    fn figures_view(&self, index: usize, view: ViewOptions) -> Result<StageView, GenerationError> {
        let stage = self.stage(index)?;
        let before = self.tags_before(index);
        let state = State::with_tags(&before);
        let generator = self.generator();
        let mut fields = Vec::new();
        for def in &FIGURE_FIELDS {
            let (table, mut options) = generator.options_for(&TableId::new(def.table), &state)?;
            if !view.show_tags {
                options.iter_mut().for_each(|o| o.tags.clear());
            }
            fields.push(FigureFieldOptions {
                field: def.id.to_owned(),
                label: def.label.to_owned(),
                table,
                options,
            });
        }
        let (_, relationship_options) =
            generator.options_for(&TableId::new(RELATIONSHIP_TABLE), &state)?;
        let (status, result, skipped, resolution) = match &stage.state {
            StageState::Pending => (StageStatus::Pending, None, None, None),
            StageState::Skipped(reason) => (StageStatus::Skipped, None, Some(*reason), None),
            StageState::Resolved(res) => {
                (StageStatus::Resolved, res.main_result(), None, Some(res))
            }
        };
        let figure_generator = FigureGenerator {
            generator: &generator,
            content: &self.content,
            settlement_tags: &before,
        };
        let name_parts: Vec<Vec<NamePartOptions>> = resolution
            .and_then(|r| r.figures.as_deref())
            .map(|f| {
                f.figures
                    .iter()
                    .map(|fig| {
                        let mut parts = figure_generator.name_part_options(fig);
                        if !view.show_tags {
                            for p in &mut parts {
                                p.options.iter_mut().for_each(|o| o.tags.clear());
                            }
                        }
                        parts
                    })
                    .collect()
            })
            .unwrap_or_default();
        let record = resolution.and_then(|r| r.figures.as_deref()).map(|f| {
            let mut f = f.clone();
            if !view.show_tags {
                f.figures.iter_mut().for_each(|fig| fig.tags.clear());
            }
            f
        });
        let table = Named {
            id: TableId::new(FIGURES_STAGE_ID),
            name: stage.label.clone(),
        };
        Ok(StageView {
            index,
            stage: table.clone(),
            table,
            status,
            current: self.mode == GenerationMode::Incremental && index == self.cursor,
            actionable: self.check_access(index).is_ok(),
            can_roll: self.representatives_before(index).is_some(),
            dice: None,
            incomplete: None,
            options: Vec::new(),
            result,
            previous: stage.previous.clone(),
            skipped,
            outcome: None,
            // The stage adds no settlement tags; each figure carries its own.
            tags: view.show_tags.then(Vec::new),
            details: None,
            naming: None,
            sub_stage: stage.sub_stage,
            roles: None,
            figures: Some(FiguresView {
                record,
                fields,
                relationship_options,
                name_parts,
            }),
        })
    }
}

/// The Authority Figures stage as saved: its figures restored exactly,
/// skipped if it did not apply, or "not recorded" for settlements saved
/// before Authority Figures existed. Nothing is regenerated here.
fn restored_figures(saved: Option<SnapshotStage>) -> StageState {
    let Some(saved) = saved else {
        return StageState::Skipped(SkipReason::NotRecorded);
    };
    match saved.figures.clone() {
        Some(figures) => StageState::Resolved(Box::new(Resolution {
            input_tags: Vec::new(),
            events: Vec::new(),
            activated: Vec::new(),
            contexts: Some(Vec::new()),
            original: Some(saved),
            name: None,
            figures: Some(Box::new(figures)),
            roles: None,
        })),
        None if saved.skipped => StageState::Skipped(SkipReason::NotEligible),
        None => StageState::Pending,
    }
}

// ----- Role Allocation --------------------------------------------------------------

/// What the Role Allocation stage before a stage holds.
enum RolesBefore {
    /// Allocated roles.
    Allocated(RoleAllocation),
    /// A Role Allocation stage is still to be resolved.
    Pending,
    /// No roles: no Role Allocation stage, or none recorded or applicable.
    None,
}

impl GenerationSession {
    /// Rolls one role again (by position: the lead first), checked against
    /// the other roles held. A unique role already held becomes its
    /// fallback.
    pub fn roll_role(
        &mut self,
        index: usize,
        slot: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_roles(index, |a, record| a.set_role(record, slot, None, faces))
    }

    /// Chooses one role. Choosing a unique role another figure holds gives
    /// its fallback, as a roll would.
    pub fn choose_role(
        &mut self,
        index: usize,
        slot: usize,
        result: ResultId,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        self.change_roles(index, |a, record| {
            a.set_role(record, slot, Some(result), faces)
        })
    }

    fn change_roles(
        &mut self,
        index: usize,
        change: impl FnOnce(&RoleAllocator<'_, '_>, &mut RoleAllocation) -> Result<(), GenerationError>,
    ) -> Result<StageChange, GenerationError> {
        self.check_access(index)?;
        let stage = self.stage(index)?;
        if !stage.is_roles() {
            return Err(GenerationError::NotRolesStage { stage: index });
        }
        let StageState::Resolved(res) = &stage.state else {
            return Err(GenerationError::RolesNotAllocated);
        };
        let mut record = res
            .roles
            .as_deref()
            .cloned()
            .ok_or(GenerationError::RolesNotAllocated)?;
        let before = self.tags_before(index);
        let generator = self.generator();
        change(
            &RoleAllocator {
                generator: &generator,
                content: &self.content,
                settlement_tags: &before,
            },
            &mut record,
        )?;
        if let StageState::Resolved(res) = &mut self.stages[index].state {
            res.roles = Some(Box::new(record));
            res.original = None;
        }
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// Allocates every role. Skipped when there is no Authority.
    fn resolve_roles(
        &mut self,
        index: usize,
        faces: &mut dyn FaceSource,
    ) -> Result<StageChange, GenerationError> {
        let before = self.tags_before(index);
        let state = match self.representatives_before(index) {
            Some(request) => {
                let generator = self.generator();
                let record = RoleAllocator {
                    generator: &generator,
                    content: &self.content,
                    settlement_tags: &before,
                }
                .allocate(&request, faces)?;
                StageState::Resolved(Box::new(Resolution {
                    input_tags: before,
                    events: Vec::new(),
                    activated: Vec::new(),
                    contexts: Some(Vec::new()),
                    original: None,
                    name: None,
                    figures: None,
                    roles: Some(Box::new(record)),
                }))
            }
            None if self.settled_before(index) => StageState::Skipped(SkipReason::NotEligible),
            None => return Err(GenerationError::RolesNeedAuthority),
        };
        let stage = &mut self.stages[index];
        stage.state = state;
        stage.previous = None;
        let (invalidated, reopened) = self.refresh(index + 1)?;
        Ok(StageChange {
            stage: index,
            invalidated,
            reopened,
        })
    }

    /// The Role Allocation before `index`, if the sequence has one.
    fn roles_before(&self, index: usize) -> RolesBefore {
        for stage in self.stages[..index].iter().rev() {
            if !stage.is_roles() {
                continue;
            }
            return match &stage.state {
                StageState::Resolved(res) => res
                    .roles
                    .as_deref()
                    .cloned()
                    .map_or(RolesBefore::None, RolesBefore::Allocated),
                StageState::Pending => RolesBefore::Pending,
                StageState::Skipped(_) => RolesBefore::None,
            };
        }
        RolesBefore::None
    }

    /// Re-checks the Role Allocation stage. Roles stand while the Authority,
    /// its size and its role scheme are unchanged; otherwise they are
    /// invalidated. Returns `Some(true)` if invalidated, `Some(false)` if
    /// reopened.
    fn refresh_roles(&mut self, index: usize) -> Option<bool> {
        let request = self.representatives_before(index);
        let settled = self.settled_before(index);
        let before = self.tags_before(index);
        let scheme = crate::roles::scheme_for(&self.content, &before).map(|s| s.id.clone());
        let mut outcome = None;
        match &self.stages[index].state {
            StageState::Resolved(res) => {
                let same = res.roles.as_ref().is_some_and(|r| {
                    Some(&r.request) == request.as_ref() && Some(&r.scheme.id) == scheme.as_ref()
                });
                if !same {
                    let previous = res.main_result();
                    let stage = &mut self.stages[index];
                    stage.previous = previous;
                    stage.state = StageState::Pending;
                    outcome = Some(true);
                }
            }
            StageState::Skipped(SkipReason::NotEligible) if request.is_some() => {
                self.stages[index].state = StageState::Pending;
                return Some(false);
            }
            _ => {}
        }
        if request.is_none() && settled && matches!(self.stages[index].state, StageState::Pending) {
            let stage = &mut self.stages[index];
            stage.state = StageState::Skipped(SkipReason::NotEligible);
            stage.previous = None;
        }
        outcome
    }

    fn roles_view(&self, index: usize, view: ViewOptions) -> Result<StageView, GenerationError> {
        let stage = self.stage(index)?;
        let before = self.tags_before(index);
        let generator = self.generator();
        let request = self.representatives_before(index);
        let mut slots = request
            .as_ref()
            .map(|r| {
                RoleAllocator {
                    generator: &generator,
                    content: &self.content,
                    settlement_tags: &before,
                }
                .slot_options(r)
            })
            .unwrap_or_default();
        if !view.show_tags {
            for slot in &mut slots {
                slot.options.iter_mut().for_each(|o| o.tags.clear());
            }
        }
        let (status, result, skipped, resolution) = match &stage.state {
            StageState::Pending => (StageStatus::Pending, None, None, None),
            StageState::Skipped(reason) => (StageStatus::Skipped, None, Some(*reason), None),
            StageState::Resolved(res) => {
                (StageStatus::Resolved, res.main_result(), None, Some(res))
            }
        };
        let table = Named {
            id: TableId::new(ROLES_STAGE_ID),
            name: stage.label.clone(),
        };
        Ok(StageView {
            index,
            stage: table.clone(),
            table,
            status,
            current: self.mode == GenerationMode::Incremental && index == self.cursor,
            actionable: self.check_access(index).is_ok(),
            can_roll: request.is_some() && !slots.is_empty(),
            dice: None,
            incomplete: None,
            options: Vec::new(),
            result,
            previous: stage.previous.clone(),
            skipped,
            outcome: None,
            // Roles are never settlement tags.
            tags: view.show_tags.then(Vec::new),
            details: None,
            naming: None,
            sub_stage: stage.sub_stage,
            figures: None,
            roles: Some(RolesView {
                record: resolution.and_then(|r| r.roles.as_deref()).cloned(),
                slots,
            }),
        })
    }
}

/// The Role Allocation stage as saved: its roles restored exactly, skipped
/// if it did not apply, or "not recorded" for settlements saved before
/// roles existed. Nothing is reallocated here.
fn restored_roles(saved: Option<SnapshotStage>) -> StageState {
    let Some(saved) = saved else {
        return StageState::Skipped(SkipReason::NotRecorded);
    };
    match saved.roles.clone() {
        Some(roles) => StageState::Resolved(Box::new(Resolution {
            input_tags: Vec::new(),
            events: Vec::new(),
            activated: Vec::new(),
            contexts: Some(Vec::new()),
            original: Some(saved),
            name: None,
            figures: None,
            roles: Some(Box::new(roles)),
        })),
        None if saved.skipped => StageState::Skipped(SkipReason::NotEligible),
        None => StageState::Pending,
    }
}
