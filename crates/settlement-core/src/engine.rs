//! Table-rolling engine.
//!
//! Interprets content generically: tables are rolled with their declared
//! physical dice (or chosen by the user), results emit tags, and tag effects
//! (redirects, variants, restrictions, dice changes, routing, secondary rolls,
//! added tags) and table follow-up rolls are applied as data. Every die rolled
//! is recorded, and each record copies the display names in use at the time
//! so it remains a faithful snapshot.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::content::{ContentSet, ContentStatus, Effect, FaceRange, SelectionMode, Table, Tag};
use crate::dice::{DiceExpr, DiceParseError, DiceRoll, FaceError, FaceSource};
use crate::ids::{ResultId, TableId, TagId, VariantId};
use crate::layout::Layout;
use crate::validation::{ValidationReport, validate};

/// Maximum table rolls in one generation; guards against routing loops.
pub const MAX_TABLE_ROLLS: usize = 1000;
/// Maximum rerolls of one table while looking for an eligible result.
pub const MAX_REROLLS: usize = 1000;

/// Results chosen by the user instead of rolled, keyed by the table actually
/// used (after redirects). Required for choose-only tables; optional for
/// rolled tables (manual generation).
pub type Selections = BTreeMap<TableId, ResultId>;

/// An ID together with the display name it had when recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named<T> {
    pub id: T,
    pub name: String,
}

impl<T> Named<T> {
    pub(crate) fn new(id: &T, name: &str) -> Self
    where
        T: Clone,
    {
        Named {
            id: id.clone(),
            name: name.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Accepted,
    /// The result is disabled; rolled again.
    Disabled,
    /// The result's eligibility condition did not hold; rolled again.
    Ineligible,
    /// A tag effect excluded the result; rolled again.
    Restricted,
    /// Not on the table's current (tag-selected) layout, so it cannot be
    /// chosen outside God Mode.
    NotInLayout,
}

impl fmt::Display for AttemptOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AttemptOutcome::Accepted => "accepted",
            AttemptOutcome::Disabled => "disabled",
            AttemptOutcome::Ineligible => "ineligible",
            AttemptOutcome::Restricted => "restricted",
            AttemptOutcome::NotInLayout => "not available on the current layout",
        })
    }
}

/// One physical roll on a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollAttempt {
    pub roll: DiceRoll,
    /// The face read on the table: the roll total, moved to the nearest end
    /// of the table if a modifier took it past either end.
    pub face: i32,
    pub result: Named<ResultId>,
    pub outcome: AttemptOutcome,
}

/// A table follow-up roll that was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FollowUpRecord {
    pub id: String,
    pub name: String,
    pub roll: DiceRoll,
    /// The result before the follow-up.
    pub from: Named<ResultId>,
    /// The result it changed to, or `None` if the original was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub becomes: Option<Named<ResultId>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRollRecord {
    /// The table actually used.
    pub table: Named<TableId>,
    /// The table generation asked for, when a redirect replaced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirected_from: Option<Named<TableId>>,
    /// The tag whose `route_to` effect caused this roll, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routed_by: Option<Named<TagId>>,
    /// Tags active when the table was resolved (the input to its rules).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_tags: Vec<Named<TagId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Named<VariantId>>,
    /// Dice expression actually rolled (after tag effects); `None` when the
    /// result was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dice: Option<String>,
    /// The result was chosen by the user rather than rolled.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
    /// Every roll made, the accepted one last. Empty when chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<RollAttempt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub follow_ups: Vec<FollowUpRecord>,
    /// The final result (after follow-ups).
    pub result: Named<ResultId>,
    /// Enabled tags emitted by the final result.
    pub tags: Vec<Named<TagId>>,
    /// The final result's numeric value at the time of the roll (e.g. a
    /// number of representatives), if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// Tag effects that shaped this roll (redirect, variant, dice,
    /// restrictions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<AppliedModifier>,
}

impl TableRollRecord {
    /// The accepted dice roll, if the result was rolled.
    pub fn accepted(&self) -> Option<&RollAttempt> {
        self.attempts.last()
    }
}

/// ```text
/// table: Economy
/// dice: 1d12
/// roll: 9
/// result: Prosperous
/// ```
impl fmt::Display for TableRollRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "table: {}", self.table.name)?;
        if let Some(from) = &self.redirected_from {
            writeln!(f, "instead of: {}", from.name)?;
        }
        if let Some(tag) = &self.routed_by {
            writeln!(f, "routed by: {}", tag.name)?;
        }
        if let Some(v) = &self.variant {
            writeln!(f, "variant: {}", v.name)?;
        }
        if let Some(dice) = &self.dice {
            writeln!(f, "dice: {dice}")?;
        }
        for attempt in &self.attempts {
            let face = if attempt.face == attempt.roll.total {
                String::new()
            } else {
                format!(" (read as {})", attempt.face)
            };
            match attempt.outcome {
                AttemptOutcome::Accepted => writeln!(f, "roll: {}{face}", attempt.roll)?,
                other => writeln!(
                    f,
                    "roll: {}{face} → {} ({other}, rerolled)",
                    attempt.roll, attempt.result.name,
                )?,
            }
        }
        if self.selected {
            let chosen = self.follow_ups.first().map_or(&self.result, |u| &u.from);
            writeln!(f, "chosen: {}", chosen.name)?;
        }
        for follow_up in &self.follow_ups {
            match &follow_up.becomes {
                Some(to) => writeln!(
                    f,
                    "{}: roll {} ({}) → {}",
                    follow_up.name, follow_up.roll, follow_up.roll.expression, to.name
                )?,
                None => writeln!(
                    f,
                    "{}: roll {} ({}) → keep {}",
                    follow_up.name, follow_up.roll, follow_up.roll.expression, follow_up.from.name
                )?,
            }
        }
        write!(f, "result: {}", self.result.name)
    }
}

/// A tag effect that influenced a table roll, kept so a saved record can
/// explain itself without the content that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedModifier {
    pub tag: Named<TagId>,
    pub effect: Effect,
    /// The effect was active but ignored (restrictions under God Mode).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
}

/// Options that change how generation interprets content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationOptions {
    /// Ignore tag-based result eligibility (result `eligibility` conditions
    /// and `restrict_results` effects). Disabled results stay disabled, and
    /// the chosen result still emits its tags.
    #[serde(default)]
    pub god_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Inactive,
    NotEligible,
    /// Naming stage: no naming scheme applies (a name can still be typed).
    NoNamingScheme,
    /// Naming stage of a settlement saved before naming existed.
    NotRecorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GenerationEvent {
    TableRolled(Box<TableRollRecord>),
    TableSkipped {
        table: Named<TableId>,
        reason: SkipReason,
    },
    SecondaryRoll {
        tag: Named<TagId>,
        label: String,
        roll: DiceRoll,
    },
}

/// Everything that happened in one generation, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationLog {
    pub events: Vec<GenerationEvent>,
    /// Tags active at the end, in activation order.
    pub tags: Vec<Named<TagId>>,
    /// Options the generation ran with.
    #[serde(default)]
    pub options: GenerationOptions,
}

impl GenerationLog {
    pub fn table_rolls(&self) -> impl Iterator<Item = &TableRollRecord> {
        self.events.iter().filter_map(|e| match e {
            GenerationEvent::TableRolled(r) => Some(&**r),
            _ => None,
        })
    }

    /// The (last) record for `table`, if it was rolled or chosen.
    pub fn record_for(&self, table: &TableId) -> Option<&TableRollRecord> {
        self.table_rolls().filter(|r| &r.table.id == table).last()
    }

    pub fn has_tag(&self, id: &TagId) -> bool {
        self.tags.iter().any(|t| &t.id == id)
    }
}

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("the content defines no generation stages")]
    EmptySequence,
    #[error("no naming scheme applies to this settlement yet; enter a name instead")]
    NoNamingScheme,
    #[error("no name was generated: table `{table}` could not be rolled")]
    NamingIncomplete { table: TableId },
    #[error("stage {} is not the name stage", stage + 1)]
    NotNamingStage { stage: usize },
    #[error("stage {} is not the Role Allocation stage", stage + 1)]
    NotRolesStage { stage: usize },
    #[error("roles need the Authority, its Representation and its Authority Size first")]
    RolesNeedAuthority,
    #[error("no roles have been allocated yet")]
    RolesNotAllocated,
    #[error("no role tables are configured for {authority}")]
    NoRoleScheme { authority: String },
    #[error("role scheme `{scheme}` has no {what} role table")]
    RoleTableMissing { scheme: String, what: String },
    #[error("there is no role {}", index + 1)]
    NoSuchRole { index: usize },
    #[error("the roles no longer match the Authority Figures; allocate them again")]
    RolesDoNotMatch,
    #[error("stage {} is not the Authority Figures stage", stage + 1)]
    NotFiguresStage { stage: usize },
    #[error(
        "Authority Figures need the Authority, its Representation and its Authority Size first"
    )]
    FiguresNeedAuthority,
    #[error("no Authority Figures have been generated yet")]
    FiguresNotGenerated,
    #[error("there is no Authority Figure {}", index + 1)]
    NoSuchFigure { index: usize },
    #[error("there is no relationship {}", index + 1)]
    NoSuchRelationship { index: usize },
    #[error("no name table is configured for this figure's species; enter a name instead")]
    NoFigureNamingScheme,
    #[error("this figure's name was not generated from a name table")]
    FigureNameNotGenerated,
    #[error("the name has no part {}", index + 1)]
    NoSuchNamePart { index: usize },
    #[error("Authority Figures have no field `{0}`")]
    NoSuchFigureField(String),
    #[error("table `{table}` does not apply, so it cannot give an Authority Figure's details")]
    FigureTableSkipped { table: TableId },
    #[error("{0}")]
    InvalidName(String),
    #[error("content is invalid:\n{0}")]
    InvalidContent(ValidationReport),
    #[error("table `{0}` does not exist")]
    UnknownTable(TableId),
    #[error("table `{table}` has no result that can be rolled with the active tags")]
    NoEligibleResult { table: TableId },
    #[error("table `{table}` was rerolled {MAX_REROLLS} times without an eligible result")]
    RerollLimit { table: TableId },
    #[error("generation exceeded {MAX_TABLE_ROLLS} table rolls; check for routing loops")]
    StepLimit,
    #[error("table `{table}` has no result for face {face}")]
    NoResultForFace { table: TableId, face: i32 },
    #[error("table `{table}` has no dice to roll")]
    NoDice { table: TableId },
    #[error("table `{table}` must be chosen, but no choice was given")]
    SelectionRequired { table: TableId },
    #[error("table `{table}` has no result `{result}` to choose")]
    UnknownSelection { table: TableId, result: ResultId },
    #[error("result `{result}` of table `{table}` cannot be chosen: it is {reason}")]
    SelectionNotAllowed {
        table: TableId,
        result: ResultId,
        reason: AttemptOutcome,
    },
    #[error("{name} cannot be generated yet: its content is incomplete{}", if note.is_empty() { String::new() } else { format!(" ({note})") })]
    IncompleteContent {
        table: TableId,
        name: String,
        note: String,
    },
    #[error("table `{table}` redirects back to a table already used")]
    RedirectLoop { table: TableId },
    #[error("stage {stage} does not exist")]
    NoSuchStage { stage: usize },
    #[error("stage {} is not the current stage (stage {} is)", stage + 1, current + 1)]
    NotCurrentStage { stage: usize, current: usize },
    #[error("stage {} must be rolled or chosen before moving on", stage + 1)]
    StageNotResolved { stage: usize },
    #[error("invalid dice expression `{expression}`: {error}")]
    InvalidDice {
        expression: String,
        error: DiceParseError,
    },
    #[error(transparent)]
    Dice(#[from] FaceError),
}

/// Rolls tables from a validated [`ContentSet`].
pub struct Generator<'c> {
    content: &'c ContentSet,
    options: GenerationOptions,
}

pub(crate) struct Request {
    table: TableId,
    variant: Option<VariantId>,
    via: Option<Named<TagId>>,
    /// A result chosen for this request instead of rolling.
    choice: Option<ResultId>,
}

impl Request {
    pub(crate) fn new(table: TableId) -> Self {
        Request {
            table,
            variant: None,
            via: None,
            choice: None,
        }
    }

    pub(crate) fn with_choice(mut self, choice: Option<ResultId>) -> Self {
        self.choice = choice;
        self
    }
}

/// The resolution context of one table; see [`Generator::context`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StageContext {
    pub table: TableId,
    pub skipped: Option<SkipReason>,
    pub variant: Option<VariantId>,
    pub dice: Option<String>,
    pub legal: Vec<ResultId>,
    pub follow_ups: Vec<String>,
}

/// One result of a table as it stands under the current tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultOption {
    pub result: Named<ResultId>,
    /// Faces on the current layout, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<FaceRange>,
    /// Enabled tags the result would emit.
    pub tags: Vec<Named<TagId>>,
    /// May be chosen now.
    pub legal: bool,
    /// Why it may not be chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<AttemptOutcome>,
}

#[derive(Default)]
pub(crate) struct State {
    active: Vec<TagId>,
    active_set: HashSet<TagId>,
    events: Vec<GenerationEvent>,
}

impl State {
    /// A state with `tags` already active (their effects already applied).
    pub(crate) fn with_tags<'t>(tags: impl IntoIterator<Item = &'t TagId>) -> Self {
        let mut state = State::default();
        for tag in tags {
            if state.active_set.insert(tag.clone()) {
                state.active.push(tag.clone());
            }
        }
        state
    }

    pub(crate) fn active(&self) -> &[TagId] {
        &self.active
    }

    pub(crate) fn take_events(&mut self) -> Vec<GenerationEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn events_so_far(&self) -> &[GenerationEvent] {
        &self.events
    }

    fn has(&self, tag: &TagId) -> bool {
        self.active_set.contains(tag)
    }
}

/// Everything about how a table is resolved under the active tags.
struct Prepared<'c> {
    variant: Option<&'c crate::content::TableVariant>,
    layout: Layout<'c>,
    dice: Option<&'c str>,
    modifier: i32,
    only: Option<HashSet<&'c ResultId>>,
    exclude: HashSet<&'c ResultId>,
    modifiers: Vec<AppliedModifier>,
}

impl<'c> Generator<'c> {
    /// Validates `content`; any validation error prevents generation.
    pub fn new(content: &'c ContentSet) -> Result<Self, GenerationError> {
        let report = validate(content);
        if report.has_errors() {
            return Err(GenerationError::InvalidContent(report));
        }
        Ok(Generator {
            content,
            options: GenerationOptions::default(),
        })
    }

    /// A generator for content already validated by [`Generator::new`].
    pub(crate) fn from_validated(content: &'c ContentSet, options: GenerationOptions) -> Self {
        Generator { content, options }
    }

    pub fn with_options(mut self, options: GenerationOptions) -> Self {
        self.options = options;
        self
    }

    pub fn options(&self) -> GenerationOptions {
        self.options
    }

    /// Generates a settlement: runs the content's `sequence`.
    pub fn run_sequence(
        &self,
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<GenerationLog, GenerationError> {
        self.run_with(&self.content.sequence_tables(), selections, faces)
    }

    /// Rolls each table in `start` in order, following routes depth-first.
    pub fn run(
        &self,
        start: &[TableId],
        faces: &mut dyn FaceSource,
    ) -> Result<GenerationLog, GenerationError> {
        self.run_with(start, &Selections::new(), faces)
    }

    /// Like [`run`](Self::run), using `selections` for chosen results.
    pub fn run_with(
        &self,
        start: &[TableId],
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<GenerationLog, GenerationError> {
        let mut state = State::default();
        let queue = start.iter().map(|t| Request::new(t.clone())).collect();
        self.process(&mut state, queue, selections, faces)?;
        Ok(self.log_from(state))
    }

    pub(crate) fn log_from(&self, state: State) -> GenerationLog {
        let tags = state
            .active
            .iter()
            .filter_map(|id| self.content.tag(id))
            .map(|t| Named::new(&t.id, &t.name))
            .collect();
        GenerationLog {
            events: state.events,
            tags,
            options: self.options,
        }
    }

    /// Why `table` would be skipped under the current tags, if it would.
    fn skip_reason(&self, table: &Table, state: &State) -> Option<SkipReason> {
        if !table.active {
            Some(SkipReason::Inactive)
        } else if table
            .eligibility
            .as_ref()
            .is_some_and(|c| !c.evaluate(&|t| state.has(t)))
        {
            Some(SkipReason::NotEligible)
        } else {
            None
        }
    }

    /// Resolves every queued request (and the routes they trigger),
    /// appending events to `state`.
    pub(crate) fn process(
        &self,
        state: &mut State,
        mut queue: VecDeque<Request>,
        selections: &Selections,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        let mut steps = 0;
        while let Some(request) = queue.pop_front() {
            steps += 1;
            if steps > MAX_TABLE_ROLLS {
                return Err(GenerationError::StepLimit);
            }
            let requested = self.table(&request.table)?;
            let (table, redirect) = self.resolve_redirects(requested, state)?;

            if let Some(reason) = self.skip_reason(table, state) {
                state.events.push(GenerationEvent::TableSkipped {
                    table: Named::new(&table.id, &table.name),
                    reason,
                });
                continue;
            }

            let choice = request.choice.as_ref().or(selections.get(&table.id));
            if choice.is_none() && table.status == ContentStatus::Incomplete {
                return Err(GenerationError::IncompleteContent {
                    table: table.id.clone(),
                    name: table.name.clone(),
                    note: table.status_note.clone(),
                });
            }
            let input_tags = self.named_tags(&state.active);
            let prepared = self.prepare(table, request.variant.as_ref(), state);
            let mut record = match choice {
                Some(choice) => self.select(table, choice, prepared, state)?,
                None if table.selection == SelectionMode::Choose => {
                    return Err(GenerationError::SelectionRequired {
                        table: table.id.clone(),
                    });
                }
                None => self.roll(table, prepared, state, faces)?,
            };
            if !std::ptr::eq(requested, table) {
                record.redirected_from = Some(Named::new(&requested.id, &requested.name));
            }
            record.modifiers.splice(0..0, redirect);
            record.routed_by = request.via;
            record.input_tags = input_tags;
            self.apply_follow_ups(table, &mut record, state, faces)?;

            let result = table
                .result(&record.result.id)
                .expect("records refer to table results");
            record.tags = self.enabled_tags(&result.tags);
            record.value = result.value;
            let emitted = result.tags.clone();
            state
                .events
                .push(GenerationEvent::TableRolled(Box::new(record)));
            let routes = self.emit(state, &emitted, faces)?;
            for (i, route) in routes.into_iter().enumerate() {
                queue.insert(i, route);
            }
        }
        Ok(())
    }

    /// Tags activated when `tags` are emitted: enabled tags plus their
    /// `add_tag` chains, in emission order (no dice, no side effects).
    pub(crate) fn activation_closure(&self, tags: &[TagId]) -> Vec<TagId> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut pending: VecDeque<TagId> = tags.iter().cloned().collect();
        while let Some(id) = pending.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(tag) = self.content.tag(&id).filter(|t| t.enabled) else {
                continue;
            };
            out.push(id);
            for def in tag.effects.iter().filter(|d| d.is_active()) {
                if let Effect::AddTag { tag } = &def.effect {
                    pending.push_back(tag.clone());
                }
            }
        }
        out
    }

    fn named_tags(&self, ids: &[TagId]) -> Vec<Named<TagId>> {
        ids.iter()
            .filter_map(|id| self.content.tag(id))
            .map(|t| Named::new(&t.id, &t.name))
            .collect()
    }

    /// How `requested` resolves under the tags in `state`: which table and
    /// variant are used, with which dice, which results are legal, and
    /// (given a result) which follow-up rolls apply. Two equal contexts mean
    /// a resolved stage is still valid.
    pub(crate) fn context(
        &self,
        requested: &TableId,
        state: &State,
        result: Option<&ResultId>,
    ) -> Result<StageContext, GenerationError> {
        let requested = self.table(requested)?;
        let (table, _) = self.resolve_redirects(requested, state)?;
        if let Some(reason) = self.skip_reason(table, state) {
            return Ok(StageContext {
                table: table.id.clone(),
                skipped: Some(reason),
                ..StageContext::default()
            });
        }
        let prepared = self.prepare(table, None, state);
        let options = self.result_options(table, &prepared, state);
        let follow_ups = result
            .map(|r| {
                table
                    .follow_ups
                    .iter()
                    .filter(|f| {
                        f.on_results.contains(r)
                            && f.when
                                .as_ref()
                                .is_none_or(|c| c.evaluate(&|t| state.has(t)))
                    })
                    .map(|f| f.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        Ok(StageContext {
            table: table.id.clone(),
            skipped: None,
            variant: prepared.variant.map(|v| v.id.clone()),
            dice: prepared
                .dice
                .map(|d| {
                    parse_validated(d).map(|e| e.with_extra_modifier(prepared.modifier).to_string())
                })
                .transpose()?,
            legal: options
                .iter()
                .filter(|o| o.legal)
                .map(|o| o.result.id.clone())
                .collect(),
            follow_ups,
        })
    }

    /// Every result of `requested` under the tags in `state`, with whether it
    /// may currently be chosen and why not.
    pub(crate) fn options_for(
        &self,
        requested: &TableId,
        state: &State,
    ) -> Result<(Named<TableId>, Vec<ResultOption>), GenerationError> {
        let requested = self.table(requested)?;
        let (table, _) = self.resolve_redirects(requested, state)?;
        let prepared = self.prepare(table, None, state);
        Ok((
            Named::new(&table.id, &table.name),
            self.result_options(table, &prepared, state),
        ))
    }

    fn result_options(
        &self,
        table: &Table,
        prepared: &Prepared<'_>,
        state: &State,
    ) -> Vec<ResultOption> {
        table
            .results
            .iter()
            .map(|r| {
                let outcome = self.outcome_for(table, prepared, state, &r.id);
                let range = prepared
                    .layout
                    .entries
                    .iter()
                    .find(|(id, _)| *id == &r.id)
                    .map(|(_, range)| *range);
                let reason = if outcome != AttemptOutcome::Accepted {
                    Some(outcome)
                } else if range.is_none() && self.layout_limits_choices(table) {
                    Some(AttemptOutcome::NotInLayout)
                } else {
                    None
                };
                ResultOption {
                    result: Named::new(&r.id, &r.name),
                    range,
                    tags: self.enabled_tags(&r.tags),
                    legal: reason.is_none(),
                    reason,
                }
            })
            .collect()
    }

    /// Whether only results on the current layout may be chosen. Not for
    /// choose-only or dice-less tables, and not in God Mode (a tag-driven
    /// layout is an eligibility limit God Mode removes).
    fn layout_limits_choices(&self, table: &Table) -> bool {
        !self.options.god_mode && table.selection == SelectionMode::Roll && table.dice.is_some()
    }

    fn table(&self, id: &TableId) -> Result<&'c Table, GenerationError> {
        self.content
            .table(id)
            .ok_or_else(|| GenerationError::UnknownTable(id.clone()))
    }

    fn enabled_tags(&self, ids: &[TagId]) -> Vec<Named<TagId>> {
        ids.iter()
            .filter_map(|id| self.content.tag(id))
            .filter(|t| t.enabled)
            .map(|t| Named::new(&t.id, &t.name))
            .collect()
    }

    /// Follows `redirect_table` effects of active tags (earliest activated
    /// first) until no redirect applies.
    fn resolve_redirects(
        &self,
        requested: &'c Table,
        state: &State,
    ) -> Result<(&'c Table, Vec<AppliedModifier>), GenerationError> {
        let mut current = requested;
        let mut visited = vec![&requested.id];
        let mut applied = Vec::new();
        loop {
            let next = self
                .effects_on(&current.id, state)
                .into_iter()
                .find_map(|(tag, effect)| match effect {
                    Effect::RedirectTable { to, .. } => Some((tag, effect, to)),
                    _ => None,
                });
            let Some((tag, effect, to)) = next else {
                return Ok((current, applied));
            };
            if visited.contains(&to) {
                return Err(GenerationError::RedirectLoop {
                    table: current.id.clone(),
                });
            }
            applied.push(AppliedModifier {
                tag: Named::new(&tag.id, &tag.name),
                effect: effect.clone(),
                ignored: false,
            });
            current = self.table(to)?;
            visited.push(&current.id);
        }
    }

    /// Emits `tags` (and any tags they add), activating them and running
    /// their per-emission effects. Returns the routes to follow, in order.
    fn emit(
        &self,
        state: &mut State,
        tags: &[TagId],
        faces: &mut dyn FaceSource,
    ) -> Result<Vec<Request>, GenerationError> {
        let mut routes = Vec::new();
        let mut seen = HashSet::new();
        let mut pending: VecDeque<TagId> = tags.iter().cloned().collect();
        while let Some(id) = pending.pop_front() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(tag) = self.content.tag(&id).filter(|t| t.enabled) else {
                continue;
            };
            if state.active_set.insert(id.clone()) {
                state.active.push(id.clone());
            }
            for def in tag.effects.iter().filter(|d| d.is_active()) {
                match &def.effect {
                    Effect::RouteTo { table, variant } => routes.push(Request {
                        table: table.clone(),
                        variant: variant.clone(),
                        via: Some(Named::new(&tag.id, &tag.name)),
                        choice: None,
                    }),
                    Effect::SecondaryRoll { label, dice } => {
                        let expr = parse_validated(dice)?;
                        let roll = expr.roll(faces)?;
                        state.events.push(GenerationEvent::SecondaryRoll {
                            tag: Named::new(&tag.id, &tag.name),
                            label: label.clone(),
                            roll,
                        });
                    }
                    Effect::AddTag { tag } => pending.push_back(tag.clone()),
                    // Applied when the target table is rolled.
                    Effect::UseVariant { .. }
                    | Effect::RestrictResults { .. }
                    | Effect::ModifyDice { .. }
                    | Effect::RedirectTable { .. } => {}
                    // Never executable (validation keeps intents planned).
                    Effect::Intent { .. } => {}
                }
            }
        }
        Ok(routes)
    }

    /// Effects of active tags that target `table`, in tag activation order.
    fn effects_on<'s>(
        &'s self,
        table: &'s TableId,
        state: &'s State,
    ) -> Vec<(&'c Tag, &'c Effect)> {
        state
            .active
            .iter()
            .filter_map(|id| self.content.tag(id))
            .flat_map(|t| t.effects.iter().map(move |d| (t, d)))
            .filter(|(_, d)| d.is_active())
            .map(|(t, d)| (t, &d.effect))
            .filter(|(_, e)| e.target_table() == Some(table))
            .collect()
    }

    /// Resolves the variant, dice and restrictions that apply to `table`.
    fn prepare(
        &self,
        table: &'c Table,
        forced_variant: Option<&VariantId>,
        state: &State,
    ) -> Prepared<'c> {
        let has = |t: &TagId| state.has(t);
        let effects = self.effects_on(&table.id, state);
        let god_mode = self.options.god_mode;
        let mut modifiers = Vec::new();
        let mut applied = |tag: &Tag, effect: &Effect, ignored: bool| {
            modifiers.push(AppliedModifier {
                tag: Named::new(&tag.id, &tag.name),
                effect: effect.clone(),
                ignored,
            });
        };

        // Variant: forced by a route, else requested by a tag, else the first
        // whose condition holds, else the base layout.
        let requested = effects.iter().find_map(|(t, e)| match e {
            Effect::UseVariant { variant, .. } => Some((*t, *e, variant)),
            _ => None,
        });
        let variant = match (forced_variant, requested) {
            (Some(id), _) => table.variant(id),
            (None, Some((tag, effect, id))) => {
                applied(tag, effect, false);
                table.variant(id)
            }
            (None, None) => None,
        }
        .or_else(|| {
            table
                .variants
                .iter()
                .find(|v| v.when.as_ref().is_some_and(|c| c.evaluate(&has)))
        });
        let layout = match variant {
            Some(v) => Layout::for_variant(table, v),
            None => Layout::base(table),
        };

        // Dice: the earliest replacement wins; modifiers add up.
        let mut dice = layout.dice;
        let mut replaced = false;
        let mut modifier = 0;
        for (tag, effect) in &effects {
            if let Effect::ModifyDice {
                dice: replacement,
                modifier: delta,
                ..
            } = effect
            {
                let replaces = replacement.is_some() && !replaced;
                if let Some(d) = replacement.as_deref().filter(|_| replaces) {
                    dice = Some(d);
                    replaced = true;
                }
                modifier += delta;
                if replaces || *delta != 0 {
                    applied(tag, effect, false);
                }
            }
        }

        // Restrictions: `only` lists intersect, `exclude` lists combine.
        // God Mode ignores them (and result eligibility) but records them.
        let mut only: Option<HashSet<&ResultId>> = None;
        let mut exclude: HashSet<&ResultId> = HashSet::new();
        for (tag, effect) in &effects {
            if let Effect::RestrictResults {
                only: o,
                exclude: x,
                ..
            } = effect
            {
                applied(tag, effect, god_mode);
                if god_mode {
                    continue;
                }
                if let Some(list) = o {
                    let set: HashSet<&ResultId> = list.iter().collect();
                    only = Some(match only {
                        Some(prev) => prev.intersection(&set).copied().collect(),
                        None => set,
                    });
                }
                exclude.extend(x.iter());
            }
        }

        Prepared {
            variant,
            layout,
            dice,
            modifier,
            only,
            exclude,
            modifiers,
        }
    }

    fn outcome_for(
        &self,
        table: &Table,
        prepared: &Prepared<'_>,
        state: &State,
        id: &ResultId,
    ) -> AttemptOutcome {
        let Some(result) = table.result(id) else {
            return AttemptOutcome::Ineligible;
        };
        if !result.enabled {
            AttemptOutcome::Disabled
        } else if prepared.exclude.contains(id)
            || prepared.only.as_ref().is_some_and(|o| !o.contains(id))
        {
            AttemptOutcome::Restricted
        } else if !self.options.god_mode
            && result
                .eligibility
                .as_ref()
                .is_some_and(|c| !c.evaluate(&|t| state.has(t)))
        {
            AttemptOutcome::Ineligible
        } else {
            AttemptOutcome::Accepted
        }
    }

    fn select(
        &self,
        table: &'c Table,
        choice: &ResultId,
        prepared: Prepared<'c>,
        state: &State,
    ) -> Result<TableRollRecord, GenerationError> {
        let result = table
            .result(choice)
            .ok_or_else(|| GenerationError::UnknownSelection {
                table: table.id.clone(),
                result: choice.clone(),
            })?;
        let outcome = self.outcome_for(table, &prepared, state, choice);
        let in_layout = prepared.layout.entries.iter().any(|(id, _)| *id == choice);
        let outcome = if outcome == AttemptOutcome::Accepted
            && !in_layout
            && self.layout_limits_choices(table)
        {
            AttemptOutcome::NotInLayout
        } else {
            outcome
        };
        if outcome != AttemptOutcome::Accepted {
            return Err(GenerationError::SelectionNotAllowed {
                table: table.id.clone(),
                result: choice.clone(),
                reason: outcome,
            });
        }
        Ok(TableRollRecord {
            table: Named::new(&table.id, &table.name),
            redirected_from: None,
            routed_by: None,
            input_tags: Vec::new(),
            variant: prepared.variant.map(|v| Named::new(&v.id, &v.name)),
            dice: None,
            selected: true,
            attempts: Vec::new(),
            follow_ups: Vec::new(),
            result: Named::new(&result.id, &result.name),
            tags: Vec::new(),
            value: None,
            modifiers: prepared.modifiers,
        })
    }

    fn roll(
        &self,
        table: &'c Table,
        prepared: Prepared<'c>,
        state: &State,
        faces: &mut dyn FaceSource,
    ) -> Result<TableRollRecord, GenerationError> {
        let dice = prepared.dice.ok_or_else(|| GenerationError::NoDice {
            table: table.id.clone(),
        })?;
        let mut expr = parse_validated(dice)?;
        if prepared.modifier != 0 {
            expr = expr.with_extra_modifier(prepared.modifier);
        }

        let no_eligible = || GenerationError::NoEligibleResult {
            table: table.id.clone(),
        };
        let layout = &prepared.layout;
        let bounds = layout.bounds().ok_or_else(no_eligible)?;
        let reachable = FaceRange::new(
            expr.min_total().clamp(bounds.min, bounds.max),
            expr.max_total().clamp(bounds.min, bounds.max),
        );
        let any_eligible = layout.entries.iter().any(|(id, range)| {
            self.outcome_for(table, &prepared, state, id) == AttemptOutcome::Accepted
                && range.intersection(&reachable).is_some()
        });
        if !any_eligible {
            return Err(no_eligible());
        }

        let mut attempts = Vec::new();
        for _ in 0..MAX_REROLLS {
            let roll = expr.roll(faces)?;
            let face = roll.total.clamp(bounds.min, bounds.max);
            let result_id =
                layout
                    .result_at(face)
                    .ok_or_else(|| GenerationError::NoResultForFace {
                        table: table.id.clone(),
                        face,
                    })?;
            let result = table.result(result_id).expect("layout refers to results");
            let outcome = self.outcome_for(table, &prepared, state, result_id);
            attempts.push(RollAttempt {
                roll,
                face,
                result: Named::new(&result.id, &result.name),
                outcome,
            });
            if outcome == AttemptOutcome::Accepted {
                return Ok(TableRollRecord {
                    table: Named::new(&table.id, &table.name),
                    redirected_from: None,
                    routed_by: None,
                    input_tags: Vec::new(),
                    variant: prepared.variant.map(|v| Named::new(&v.id, &v.name)),
                    dice: Some(expr.to_string()),
                    selected: false,
                    attempts,
                    follow_ups: Vec::new(),
                    result: Named::new(&result.id, &result.name),
                    tags: Vec::new(),
                    value: None,
                    modifiers: prepared.modifiers,
                });
            }
        }
        Err(GenerationError::RerollLimit {
            table: table.id.clone(),
        })
    }

    /// Applies the table's follow-up rolls, in order, to the result so far.
    fn apply_follow_ups(
        &self,
        table: &Table,
        record: &mut TableRollRecord,
        state: &State,
        faces: &mut dyn FaceSource,
    ) -> Result<(), GenerationError> {
        for follow_up in &table.follow_ups {
            let applies = follow_up.on_results.contains(&record.result.id)
                && follow_up
                    .when
                    .as_ref()
                    .is_none_or(|c| c.evaluate(&|t| state.has(t)));
            if !applies {
                continue;
            }
            let roll = parse_validated(&follow_up.dice)?.roll(faces)?;
            let outcome = follow_up
                .outcomes
                .iter()
                .find(|o| o.range.contains(roll.total))
                .ok_or_else(|| GenerationError::NoResultForFace {
                    table: table.id.clone(),
                    face: roll.total,
                })?;
            let becomes = outcome
                .becomes
                .as_ref()
                .and_then(|id| table.result(id))
                .map(|r| Named::new(&r.id, &r.name));
            let from = record.result.clone();
            if let Some(to) = &becomes {
                record.result = to.clone();
            }
            record.follow_ups.push(FollowUpRecord {
                id: follow_up.id.clone(),
                name: follow_up.name.clone(),
                roll,
                from,
                becomes,
            });
        }
        Ok(())
    }
}

/// Dice were validated by [`Generator::new`]; this cannot fail in practice
/// but never panics if it does.
fn parse_validated(dice: &str) -> Result<DiceExpr, GenerationError> {
    DiceExpr::parse(dice).map_err(|error| GenerationError::InvalidDice {
        expression: dice.to_owned(),
        error,
    })
}
