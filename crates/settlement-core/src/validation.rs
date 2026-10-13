//! Content validation shared by every interface.
//!
//! [`validate`] never panics on malformed content; every problem becomes a
//! [`ValidationIssue`] with a location and a human-readable message.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::content::{
    CONTENT_SCHEMA_VERSION, Condition, ContentSet, ContentStatus, Effect, FaceRange, SelectionMode,
    StepTarget, Table,
};
use crate::dice::{DiceExpr, DiceParseError};
use crate::ids::{
    LocationSchemeId, NamingSchemeId, ResultId, RoleSchemeId, TableId, TagId, VariantId,
    is_valid_id,
};
use crate::layout::Layout;
use crate::naming::JoinStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Content cannot be used for generation.
    Error,
    /// Content is usable but probably not what the author intended.
    Warning,
}

/// Where an issue was found. Unset fields are not relevant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<TableId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<ResultId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<TagId>,
    /// Index into the tag's `effects`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    /// Index into the content's generation `sequence`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_step: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_scheme: Option<NamingSchemeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_scheme: Option<RoleSchemeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_scheme: Option<LocationSchemeId>,
}

impl Location {
    pub fn naming_scheme(id: &NamingSchemeId) -> Self {
        Location {
            naming_scheme: Some(id.clone()),
            ..Default::default()
        }
    }

    pub fn table(id: &TableId) -> Self {
        Location {
            table: Some(id.clone()),
            ..Default::default()
        }
    }

    pub fn tag(id: &TagId) -> Self {
        Location {
            tag: Some(id.clone()),
            ..Default::default()
        }
    }

    pub fn with_result(mut self, id: &ResultId) -> Self {
        self.result = Some(id.clone());
        self
    }

    pub fn with_variant(mut self, id: Option<&VariantId>) -> Self {
        self.variant = id.cloned();
        self
    }

    pub fn with_effect(mut self, index: usize) -> Self {
        self.effect = Some(index);
        self
    }

    pub fn with_follow_up(mut self, id: &str) -> Self {
        self.follow_up = Some(id.to_owned());
        self
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(n) = &self.naming_scheme {
            parts.push(format!("naming scheme `{n}`"));
        }
        if let Some(t) = &self.table {
            parts.push(format!("table `{t}`"));
        }
        if let Some(v) = &self.variant {
            parts.push(format!("variant `{v}`"));
        }
        if let Some(r) = &self.result {
            parts.push(format!("result `{r}`"));
        }
        if let Some(t) = &self.tag {
            parts.push(format!("tag `{t}`"));
        }
        if let Some(e) = self.effect {
            parts.push(format!("effect #{}", e + 1));
        }
        if let Some(f) = &self.follow_up {
            parts.push(format!("follow-up `{f}`"));
        }
        if let Some(s) = self.sequence_step {
            parts.push(format!("generation sequence step {}", s + 1));
        }
        if parts.is_empty() {
            f.write_str("content")
        } else {
            f.write_str(&parts.join(" › "))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdScope {
    Table,
    Tag,
    Result,
    Variant,
    /// A result assigned more than once in one variant's ranges.
    VariantRange,
    FollowUp,
    NamingScheme,
    NamingMethod,
    RoleScheme,
    LocationScheme,
}

impl fmt::Display for IdScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IdScope::Table => "table",
            IdScope::Tag => "tag",
            IdScope::Result => "result",
            IdScope::Variant => "variant",
            IdScope::VariantRange => "variant range for result",
            IdScope::FollowUp => "follow-up roll",
            IdScope::NamingScheme => "naming scheme",
            IdScope::NamingMethod => "naming method for result",
            IdScope::RoleScheme => "role scheme",
            IdScope::LocationScheme => "location scheme",
        })
    }
}

/// Machine-readable description of a problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum IssueKind {
    UnsupportedSchemaVersion {
        found: u32,
        supported: u32,
    },
    InvalidId {
        id: String,
    },
    DuplicateId {
        scope: IdScope,
        id: String,
    },
    InvalidDice {
        expression: String,
        error: DiceParseError,
    },
    /// `min > max`, or faces the dice can never produce.
    ImpossibleRange {
        range: FaceRange,
        possible: Option<FaceRange>,
    },
    /// `first` and `second` name results (or follow-up outcomes).
    OverlappingRanges {
        first: String,
        second: String,
        overlap: FaceRange,
    },
    UncoveredOutcomes {
        dice: String,
        outcomes: Vec<FaceRange>,
    },
    MissingTag {
        tag: TagId,
    },
    MissingTable {
        table: TableId,
    },
    MissingVariant {
        table: TableId,
        variant: VariantId,
    },
    BrokenResultReference {
        table: TableId,
        result: ResultId,
    },
    /// Tables that route into each other. `guaranteed` means every result
    /// continues the cycle, so generation can never finish.
    RoutingCycle {
        tables: Vec<TableId>,
        guaranteed: bool,
    },
    /// A result with no faces in the base layout or any variant.
    UnreachableResult,
    /// A result of an `ordered` table whose base faces are not above those
    /// of the results before it, so dice modifiers no longer follow the
    /// scale.
    ScaleOutOfOrder,
    /// A result value that is not a positive integer.
    InvalidValue {
        value: i64,
    },
    /// A result without a value in a table whose other results have one.
    MissingValue,
    /// A sub-stage with no stage before it to belong to.
    SubStageWithoutParent,
    /// A unique result without a fallback to resolve as when it is held.
    UniqueWithoutFallback,
    /// A fallback on a result that is not unique.
    FallbackWithoutUnique,
    /// A fallback that is itself unique, so it could also be held.
    FallbackNotRepeatable,
    /// A role scheme with neither a single table nor a lead and member
    /// table, or with only one of lead and member.
    RoleSchemeIncomplete,
    /// A user-created item whose ID is outside the reserved `user.`
    /// namespace, so a future default could collide with it.
    NotUserId {
        id: String,
    },
    /// A user override targets a bundled default that no longer exists
    /// (e.g. it was removed by an update). The override is ignored.
    OverrideTargetMissing,
    /// The bundled default changed after the user customised it. The
    /// customisation still applies.
    DefaultChanged,
    /// A rolled table without a dice expression.
    MissingDice,
    /// Content explicitly marked as not final.
    IncompleteContent {
        note: String,
    },
    /// An `intent` effect marked active. Intents have no approved numbers
    /// and can never be executed.
    IntentNotPlanned,
    /// Tables that redirect into each other.
    RedirectCycle {
        tables: Vec<TableId>,
    },
    /// A result of a naming scheme's method table with no method defined,
    /// so rolling it could not produce a name.
    NamingMethodMissing,
    /// A naming method with no component tables.
    NamingMethodEmpty,
    /// A spelling exception that lists a different number of results than
    /// the method has components.
    NamingExceptionLength {
        components: usize,
        results: usize,
    },
    /// A join replacement with nothing to replace.
    NamingReplacementEmpty,
    /// A naming method whose `parts` list a different number of parts than
    /// it has components.
    NamingPartsLength {
        components: usize,
        parts: usize,
    },
    /// A spelling exception that lists both result IDs and display names,
    /// or neither.
    NamingExceptionForm,
    /// A spelling exception with no name to use.
    NamingExceptionEmpty,
    /// A spelling exception written by display name whose name does not
    /// pick out exactly one result of its component table.
    NamingExceptionName {
        table: TableId,
        name: String,
        matches: usize,
    },
    /// A `curated_variant` join that reads a component the method does not
    /// have, or reads the source and the variant from the same component.
    CuratedVariantComponents {
        source: usize,
        variant: usize,
        components: usize,
    },
    /// A `curated_variant` join reading a result field that holds no
    /// approved names.
    CuratedVariantField {
        field: String,
    },
    /// A source result with no approved name (or an empty one) for one of
    /// the variant table's results.
    ApprovedNameMissing {
        variant_table: TableId,
        variant: ResultId,
        variant_name: String,
    },
    /// An approved name keyed by an ID that is not a result of the variant
    /// table, so it can never be selected.
    ApprovedNameUnknownVariant {
        variant_table: TableId,
        variant: ResultId,
    },
}

impl fmt::Display for IssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueKind::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "content schema version {found} is not supported (expected {supported})"
            ),
            IssueKind::InvalidId { id } => write!(
                f,
                "`{id}` is not a valid ID (use ASCII letters, digits, `_`, `.`, `-`)"
            ),
            IssueKind::DuplicateId { scope, id } => write!(f, "duplicate {scope} ID `{id}`"),
            IssueKind::InvalidDice { expression, error } => {
                write!(f, "invalid dice expression `{expression}`: {error}")
            }
            IssueKind::ImpossibleRange { range, possible } => match possible {
                _ if range.is_empty() => write!(
                    f,
                    "range [{}, {}] is backwards (min is greater than max)",
                    range.min, range.max
                ),
                Some(p) => write!(f, "range {range} cannot be rolled (dice give {p})"),
                None => write!(f, "range {range} cannot be rolled"),
            },
            IssueKind::OverlappingRanges {
                first,
                second,
                overlap,
            } => write!(f, "results `{first}` and `{second}` both claim {overlap}"),
            IssueKind::UncoveredOutcomes { dice, outcomes } => {
                let list: Vec<String> = outcomes.iter().map(ToString::to_string).collect();
                write!(f, "no result for {} on {dice}", list.join(", "))
            }
            IssueKind::MissingTag { tag } => write!(f, "references missing tag `{tag}`"),
            IssueKind::MissingTable { table } => write!(f, "references missing table `{table}`"),
            IssueKind::MissingVariant { table, variant } => {
                write!(f, "table `{table}` has no variant `{variant}`")
            }
            IssueKind::BrokenResultReference { table, result } => {
                write!(f, "table `{table}` has no result `{result}`")
            }
            IssueKind::RoutingCycle { tables, guaranteed } => {
                let path: Vec<&str> = tables.iter().map(TableId::as_str).collect();
                if *guaranteed {
                    write!(
                        f,
                        "tables route into each other forever: {}",
                        path.join(" → ")
                    )
                } else {
                    write!(
                        f,
                        "tables can route into each other repeatedly: {}",
                        path.join(" → ")
                    )
                }
            }
            IssueKind::UnreachableResult => {
                f.write_str("result has no faces in any layout and can never be rolled")
            }
            IssueKind::InvalidValue { value } => {
                write!(f, "result value {value} must be a positive whole number")
            }
            IssueKind::MissingValue => f.write_str(
                "result has no value, but other results of the table do",
            ),
            IssueKind::SubStageWithoutParent => {
                f.write_str("the first stage cannot be a sub-stage")
            }
            IssueKind::UniqueWithoutFallback => f.write_str(
                "a unique result needs a fallback: the repeatable result it becomes when already held",
            ),
            IssueKind::FallbackWithoutUnique => {
                f.write_str("only a unique result can have a fallback")
            }
            IssueKind::FallbackNotRepeatable => {
                f.write_str("a fallback must be repeatable (not unique)")
            }
            IssueKind::RoleSchemeIncomplete => f.write_str(
                "a role scheme needs a single role table, or both a lead and a member role table",
            ),
            IssueKind::ScaleOutOfOrder => f.write_str(
                "the table is an ordered scale, but this result's faces are not above those of the results before it",
            ),
            IssueKind::NotUserId { id } => write!(
                f,
                "user-created ID `{id}` must start with `{}`",
                crate::ids::USER_ID_PREFIX
            ),
            IssueKind::OverrideTargetMissing => f.write_str(
                "customisation refers to a default that no longer exists and is ignored",
            ),
            IssueKind::DefaultChanged => f.write_str(
                "the bundled default has changed since it was customised; your changes still apply",
            ),
            IssueKind::MissingDice => f.write_str("table is rolled but has no dice expression"),
            IssueKind::IncompleteContent { note } if note.is_empty() => {
                f.write_str("content is marked incomplete")
            }
            IssueKind::IncompleteContent { note } => {
                write!(f, "content is marked incomplete: {note}")
            }
            IssueKind::IntentNotPlanned => f.write_str(
                "intent effects record direction without approved numbers and must stay planned",
            ),
            IssueKind::NamingMethodMissing => f.write_str(
                "result of the naming method table has no naming method, so it could not produce a name",
            ),
            IssueKind::NamingMethodEmpty => f.write_str("naming method has no component tables"),
            IssueKind::NamingReplacementEmpty => {
                f.write_str("naming replacement has an empty \"from\" text")
            }
            IssueKind::NamingPartsLength { components, parts } => write!(
                f,
                "naming method describes {parts} part(s) but has {components} component(s)"
            ),
            IssueKind::NamingExceptionEmpty => f.write_str("spelling exception has no name"),
            IssueKind::NamingExceptionForm => f.write_str(
                "spelling exception must list either component result IDs (\"results\") or component names (\"components\"), not both or neither",
            ),
            IssueKind::NamingExceptionName {
                table,
                name,
                matches: 0,
            } => write!(
                f,
                "spelling exception names \"{name}\", which is not a result of table `{table}`"
            ),
            IssueKind::NamingExceptionName {
                table,
                name,
                matches,
            } => write!(
                f,
                "spelling exception names \"{name}\", which matches {matches} results of table `{table}`; list result IDs instead"
            ),
            IssueKind::CuratedVariantComponents {
                source,
                variant,
                components,
            } => write!(
                f,
                "curated_variant reads its source from component {source} and its variant from component {variant} (counting from 0); they must be two different components of the method's {components}"
            ),
            IssueKind::CuratedVariantField { field } => write!(
                f,
                "curated_variant reads approved names from `{field}`, but results store them in `{}`",
                crate::naming::APPROVED_NAMES_FIELD
            ),
            IssueKind::ApprovedNameMissing {
                variant_table,
                variant,
                variant_name,
            } => write!(
                f,
                "no approved name for variant \"{variant_name}\" (`{variant}` of table `{variant_table}`); add one before this table can generate names"
            ),
            IssueKind::ApprovedNameUnknownVariant {
                variant_table,
                variant,
            } => write!(
                f,
                "approved name for `{variant}`, which is not a result of table `{variant_table}`, can never be selected"
            ),
            IssueKind::NamingExceptionLength {
                components,
                results,
            } => write!(
                f,
                "spelling exception lists {results} result(s) but the method has {components} component(s)"
            ),
            IssueKind::RedirectCycle { tables } => {
                let path: Vec<&str> = tables.iter().map(TableId::as_str).collect();
                write!(f, "tables redirect into each other: {}", path.join(" → "))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub location: Location,
    #[serde(flatten)]
    pub kind: IssueKind,
    /// Human-readable summary, included so every interface shows the same
    /// wording.
    pub message: String,
}

impl ValidationIssue {
    pub(crate) fn new(severity: Severity, location: Location, kind: IssueKind) -> Self {
        let message = format!("{location}: {kind}");
        Self {
            severity,
            location,
            kind,
            message,
        }
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{level}: {}", self.message)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for issue in &self.issues {
            writeln!(f, "{issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationReport {}

/// Validates a content set. Planned effects are not checked for dangling
/// references because they are design metadata, not executable rules.
pub fn validate(content: &ContentSet) -> ValidationReport {
    let mut v = Validator {
        content,
        tables: HashMap::new(),
        tags: HashSet::new(),
        issues: Vec::new(),
    };
    v.run();
    ValidationReport { issues: v.issues }
}

struct Validator<'a> {
    content: &'a ContentSet,
    tables: HashMap<&'a TableId, &'a Table>,
    tags: HashSet<&'a TagId>,
    issues: Vec<ValidationIssue>,
}

impl<'a> Validator<'a> {
    fn error(&mut self, location: Location, kind: IssueKind) {
        self.issues
            .push(ValidationIssue::new(Severity::Error, location, kind));
    }

    fn warning(&mut self, location: Location, kind: IssueKind) {
        self.issues
            .push(ValidationIssue::new(Severity::Warning, location, kind));
    }

    fn run(&mut self) {
        let content = self.content;
        if content.schema_version != CONTENT_SCHEMA_VERSION {
            self.error(
                Location::default(),
                IssueKind::UnsupportedSchemaVersion {
                    found: content.schema_version,
                    supported: CONTENT_SCHEMA_VERSION,
                },
            );
        }

        for table in &content.tables {
            self.check_id(table.id.as_str(), Location::table(&table.id));
            if self.tables.insert(&table.id, table).is_some() {
                self.error(
                    Location::table(&table.id),
                    IssueKind::DuplicateId {
                        scope: IdScope::Table,
                        id: table.id.to_string(),
                    },
                );
            }
        }
        for tag in &content.tags {
            self.check_id(tag.id.as_str(), Location::tag(&tag.id));
            if !self.tags.insert(&tag.id) {
                self.error(
                    Location::tag(&tag.id),
                    IssueKind::DuplicateId {
                        scope: IdScope::Tag,
                        id: tag.id.to_string(),
                    },
                );
            }
        }

        for table in &content.tables {
            self.check_table(table);
        }
        for tag in &content.tags {
            self.check_tag_effects(tag);
        }
        self.check_naming_schemes();
        self.check_role_schemes();
        self.check_location_schemes();
        if content.sequence.first().is_some_and(|s| s.sub_stage) {
            self.error(
                Location {
                    sequence_step: Some(0),
                    ..Default::default()
                },
                IssueKind::SubStageWithoutParent,
            );
        }
        for (step, id) in content
            .sequence
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.table().map(|t| (i, t)))
        {
            if !self.tables.contains_key(id) {
                self.error(
                    Location {
                        sequence_step: Some(step),
                        ..Default::default()
                    },
                    IssueKind::MissingTable { table: id.clone() },
                );
            }
        }
        // The Authority Figures stage rolls its fields and relationships
        // on fixed tables; each must exist.
        for (step, _) in content
            .sequence
            .iter()
            .enumerate()
            .filter(|(_, s)| s.target == StepTarget::AuthorityFigures)
        {
            for table in crate::figures::required_tables() {
                if !self.tables.contains_key(&table) {
                    self.error(
                        Location {
                            sequence_step: Some(step),
                            ..Default::default()
                        },
                        IssueKind::MissingTable { table },
                    );
                }
            }
        }
        self.check_routing_cycles();
        self.check_redirect_cycles();
    }

    /// Redirects between tables must never loop, whichever tags are active.
    fn check_redirect_cycles(&mut self) {
        let content = self.content;
        let index: HashMap<&TableId, usize> = content
            .tables
            .iter()
            .enumerate()
            .rev()
            .map(|(i, t)| (&t.id, i))
            .collect();
        let mut edges = vec![BTreeSet::new(); content.tables.len()];
        for tag in content.tags.iter().filter(|t| t.enabled) {
            for def in tag.effects.iter().filter(|d| d.is_active()) {
                if let Effect::RedirectTable { table, to } = &def.effect
                    && let (Some(&from), Some(&to)) = (index.get(table), index.get(to))
                {
                    edges[from].insert(to);
                }
            }
        }
        for component in cyclic_components(&edges) {
            let tables: Vec<TableId> = component
                .iter()
                .map(|&i| content.tables[i].id.clone())
                .collect();
            self.error(
                Location::table(&tables[0]),
                IssueKind::RedirectCycle { tables },
            );
        }
    }

    /// Naming schemes: unique IDs, existing tables, a method for every
    /// method-table result (and only for those), components for every
    /// method, and exceptions and possessive endings that name real results.
    /// Location schemes: valid IDs, conditions and tables, and every result
    /// of a count or district table has a count (`value`).
    fn check_location_schemes(&mut self) {
        let content = self.content;
        let mut seen = HashSet::new();
        for scheme in &content.location_schemes {
            let at = Location {
                location_scheme: Some(scheme.id.clone()),
                ..Default::default()
            };
            self.check_id(scheme.id.as_str(), at.clone());
            if !seen.insert(&scheme.id) {
                self.error(
                    at.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::LocationScheme,
                        id: scheme.id.to_string(),
                    },
                );
            }
            self.check_condition(Some(&scheme.when), &at);
            for id in std::iter::once(&scheme.count).chain(scheme.districts.as_ref()) {
                let Some(table) = self.tables.get(id).copied() else {
                    self.error(at.clone(), IssueKind::MissingTable { table: id.clone() });
                    continue;
                };
                for result in table.results.iter().filter(|r| r.value.is_none()) {
                    self.error(
                        Location::table(&table.id).with_result(&result.id),
                        IssueKind::MissingValue,
                    );
                }
            }
        }
        // The Bare Minimum Buildings Tavern is a Hospitality location.
        if content
            .sequence
            .iter()
            .any(|s| s.target == StepTarget::NotableLocations)
        {
            let type_table = TableId::new(crate::locations::TYPE_TABLE);
            match self.tables.get(&type_table).copied() {
                None => self.error(
                    Location::default(),
                    IssueKind::MissingTable {
                        table: type_table.clone(),
                    },
                ),
                Some(t)
                    if t.result(&crate::locations::HOSPITALITY_TYPE.into())
                        .is_none() =>
                {
                    self.error(
                        Location::table(&type_table),
                        IssueKind::BrokenResultReference {
                            table: type_table.clone(),
                            result: crate::locations::HOSPITALITY_TYPE.into(),
                        },
                    );
                }
                Some(_) => {}
            }
        }
    }

    fn check_role_schemes(&mut self) {
        let content = self.content;
        let mut seen = HashSet::new();
        for scheme in &content.role_schemes {
            let at = Location {
                role_scheme: Some(scheme.id.clone()),
                ..Default::default()
            };
            self.check_id(scheme.id.as_str(), at.clone());
            if !seen.insert(&scheme.id) {
                self.error(
                    at.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::RoleScheme,
                        id: scheme.id.to_string(),
                    },
                );
            }
            self.check_condition(Some(&scheme.when), &at);
            for table in [&scheme.single, &scheme.lead, &scheme.member]
                .into_iter()
                .flatten()
            {
                if !self.tables.contains_key(table) {
                    self.error(
                        at.clone(),
                        IssueKind::MissingTable {
                            table: table.clone(),
                        },
                    );
                }
            }
            let group = scheme.lead.is_some() || scheme.member.is_some();
            let complete_group = scheme.lead.is_some() && scheme.member.is_some();
            if (group && !complete_group) || (!group && scheme.single.is_none()) {
                self.error(at, IssueKind::RoleSchemeIncomplete);
            }
        }
    }

    fn check_naming_schemes(&mut self) {
        let content = self.content;
        let mut seen = HashSet::new();
        for scheme in &content.naming_schemes {
            let at = Location::naming_scheme(&scheme.id);
            self.check_id(scheme.id.as_str(), at.clone());
            if !seen.insert(&scheme.id) {
                self.error(
                    at.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::NamingScheme,
                        id: scheme.id.to_string(),
                    },
                );
            }
            self.check_condition(Some(&scheme.when), &at);
            let Some(method_table) = self.tables.get(&scheme.method_table).copied() else {
                self.error(
                    at.clone(),
                    IssueKind::MissingTable {
                        table: scheme.method_table.clone(),
                    },
                );
                continue;
            };
            for result in &method_table.results {
                if scheme.method(&result.id).is_none() {
                    self.error(
                        Location {
                            table: Some(method_table.id.clone()),
                            result: Some(result.id.clone()),
                            ..at.clone()
                        },
                        IssueKind::NamingMethodMissing,
                    );
                }
            }
            let mut methods = HashSet::new();
            for method in &scheme.methods {
                let m_at = Location {
                    result: Some(method.result.clone()),
                    ..at.clone()
                };
                if !methods.insert(&method.result) {
                    self.error(
                        m_at.clone(),
                        IssueKind::DuplicateId {
                            scope: IdScope::NamingMethod,
                            id: method.result.to_string(),
                        },
                    );
                }
                if method_table.result(&method.result).is_none() {
                    self.error(
                        m_at.clone(),
                        IssueKind::BrokenResultReference {
                            table: method_table.id.clone(),
                            result: method.result.clone(),
                        },
                    );
                }
                if method.components.is_empty() {
                    self.error(m_at.clone(), IssueKind::NamingMethodEmpty);
                }
                if method.join.replacements.iter().any(|r| r.from.is_empty()) {
                    self.error(m_at.clone(), IssueKind::NamingReplacementEmpty);
                }
                let components: Vec<Option<&Table>> = method
                    .components
                    .iter()
                    .map(|id| self.tables.get(id).copied())
                    .collect();
                for (id, table) in method.components.iter().zip(&components) {
                    if table.is_none() {
                        self.error(m_at.clone(), IssueKind::MissingTable { table: id.clone() });
                    }
                }
                if !method.parts.is_empty() && method.parts.len() != components.len() {
                    self.error(
                        m_at.clone(),
                        IssueKind::NamingPartsLength {
                            components: components.len(),
                            parts: method.parts.len(),
                        },
                    );
                }
                for exception in &method.exceptions {
                    if exception.results.is_empty() == exception.components.is_empty() {
                        self.error(m_at.clone(), IssueKind::NamingExceptionForm);
                        continue;
                    }
                    if exception.name.trim().is_empty() {
                        self.error(m_at.clone(), IssueKind::NamingExceptionEmpty);
                    }
                    if exception.len() != components.len() {
                        self.error(
                            m_at.clone(),
                            IssueKind::NamingExceptionLength {
                                components: components.len(),
                                results: exception.len(),
                            },
                        );
                        continue;
                    }
                    for (name, table) in exception.components.iter().zip(&components) {
                        if let Some(table) = table {
                            let matches = table
                                .results
                                .iter()
                                .filter(|r| r.name.trim() == name.trim())
                                .count();
                            if matches != 1 {
                                self.error(
                                    m_at.clone(),
                                    IssueKind::NamingExceptionName {
                                        table: table.id.clone(),
                                        name: name.clone(),
                                        matches,
                                    },
                                );
                            }
                        }
                    }
                    for (result, table) in exception.results.iter().zip(&components) {
                        if let Some(table) = table
                            && table.result(result).is_none()
                        {
                            self.error(
                                m_at.clone(),
                                IssueKind::BrokenResultReference {
                                    table: table.id.clone(),
                                    result: result.clone(),
                                },
                            );
                        }
                    }
                }
                if let JoinStyle::CuratedVariant {
                    source_field,
                    source_from_result_id: source,
                    variant_from_result_id: variant,
                    ..
                } = &method.join.style
                {
                    self.check_curated_variant(&m_at, &components, source_field, *source, *variant);
                }
                if let JoinStyle::Possessive {
                    before: Some(before),
                    ..
                } = &method.join.style
                {
                    for result in before {
                        let known = components
                            .iter()
                            .skip(1)
                            .flatten()
                            .any(|t| t.result(result).is_some());
                        if !known {
                            let table = method
                                .components
                                .last()
                                .cloned()
                                .unwrap_or_else(|| scheme.method_table.clone());
                            self.error(
                                m_at.clone(),
                                IssueKind::BrokenResultReference {
                                    table,
                                    result: result.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    /// A `curated_variant` join: two different components, the approved
    /// names field, and for every source result a non-empty approved name
    /// for every variant result (and none for IDs that are not variants).
    /// Problems with names are reported at the source result, so its table
    /// editor shows them.
    fn check_curated_variant(
        &mut self,
        at: &Location,
        components: &[Option<&'a Table>],
        field: &str,
        source: usize,
        variant: usize,
    ) {
        if source == variant || source >= components.len() || variant >= components.len() {
            self.error(
                at.clone(),
                IssueKind::CuratedVariantComponents {
                    source,
                    variant,
                    components: components.len(),
                },
            );
            return;
        }
        if field != crate::naming::APPROVED_NAMES_FIELD {
            self.error(
                at.clone(),
                IssueKind::CuratedVariantField {
                    field: field.to_owned(),
                },
            );
            return;
        }
        let (Some(source), Some(variants)) = (components[source], components[variant]) else {
            return;
        };
        for result in &source.results {
            let r_at = Location {
                table: Some(source.id.clone()),
                result: Some(result.id.clone()),
                ..at.clone()
            };
            for v in &variants.results {
                let present = result
                    .approved_names
                    .get(&v.id)
                    .is_some_and(|n| !n.trim().is_empty());
                if !present {
                    self.error(
                        r_at.clone(),
                        IssueKind::ApprovedNameMissing {
                            variant_table: variants.id.clone(),
                            variant: v.id.clone(),
                            variant_name: v.name.clone(),
                        },
                    );
                }
            }
            for key in result.approved_names.keys() {
                if variants.result(key).is_none() {
                    self.error(
                        r_at.clone(),
                        IssueKind::ApprovedNameUnknownVariant {
                            variant_table: variants.id.clone(),
                            variant: key.clone(),
                        },
                    );
                }
            }
        }
    }

    fn check_id(&mut self, id: &str, location: Location) {
        if !is_valid_id(id) {
            self.error(location, IssueKind::InvalidId { id: id.to_owned() });
        }
    }

    fn check_tag_ref(&mut self, tag: &TagId, location: &Location) {
        if !self.tags.contains(tag) {
            self.error(location.clone(), IssueKind::MissingTag { tag: tag.clone() });
        }
    }

    fn check_condition(&mut self, condition: Option<&Condition>, location: &Location) {
        for tag in condition.map(Condition::tags).unwrap_or_default() {
            self.check_tag_ref(tag, location);
        }
    }

    fn check_table(&mut self, table: &'a Table) {
        let loc = Location::table(&table.id);
        self.check_condition(table.eligibility.as_ref(), &loc);

        let mut result_ids = HashSet::new();
        for result in &table.results {
            let rloc = loc.clone().with_result(&result.id);
            self.check_id(result.id.as_str(), rloc.clone());
            if !result_ids.insert(&result.id) {
                self.error(
                    rloc.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::Result,
                        id: result.id.to_string(),
                    },
                );
            }
            for tag in &result.tags {
                self.check_tag_ref(tag, &rloc);
            }
            self.check_condition(result.eligibility.as_ref(), &rloc);
            // No generators exist yet, so a reference cannot be resolved;
            // only its form is checked.
            if let Some(generator) = &result.generator {
                self.check_id(generator.as_str(), rloc.clone());
            }
            for variant in &result.label_variants {
                self.check_tag_ref(&variant.tag, &rloc);
            }
            match (result.unique, &result.fallback) {
                (true, None) => self.error(rloc.clone(), IssueKind::UniqueWithoutFallback),
                (false, Some(_)) => self.error(rloc.clone(), IssueKind::FallbackWithoutUnique),
                (true, Some(fallback)) => match table.result(fallback) {
                    None => self.error(
                        rloc.clone(),
                        IssueKind::BrokenResultReference {
                            table: table.id.clone(),
                            result: fallback.clone(),
                        },
                    ),
                    Some(f) if f.unique => {
                        self.error(rloc.clone(), IssueKind::FallbackNotRepeatable);
                    }
                    Some(_) => {}
                },
                (false, None) => {}
            }
            match result.value {
                Some(value) if value < 1 => {
                    self.error(rloc.clone(), IssueKind::InvalidValue { value });
                }
                None if table.results.iter().any(|r| r.value.is_some()) => {
                    self.error(rloc.clone(), IssueKind::MissingValue);
                }
                _ => {}
            }
        }

        let mut variant_ids = HashSet::new();
        for variant in &table.variants {
            let vloc = loc.clone().with_variant(Some(&variant.id));
            self.check_id(variant.id.as_str(), vloc.clone());
            if !variant_ids.insert(&variant.id) {
                self.error(
                    vloc.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::Variant,
                        id: variant.id.to_string(),
                    },
                );
            }
            self.check_condition(variant.when.as_ref(), &vloc);
            let mut assigned = HashSet::new();
            for assignment in variant.ranges.iter().flatten() {
                let aloc = vloc.clone().with_result(&assignment.result);
                if table.result(&assignment.result).is_none() {
                    self.error(
                        aloc,
                        IssueKind::BrokenResultReference {
                            table: table.id.clone(),
                            result: assignment.result.clone(),
                        },
                    );
                } else if !assigned.insert(&assignment.result) {
                    self.error(
                        aloc,
                        IssueKind::DuplicateId {
                            scope: IdScope::VariantRange,
                            id: assignment.result.to_string(),
                        },
                    );
                }
            }
        }

        self.check_follow_ups(table);

        let incomplete = table.status == ContentStatus::Incomplete;
        if incomplete {
            self.warning(
                loc.clone(),
                IssueKind::IncompleteContent {
                    note: table.status_note.clone(),
                },
            );
        }
        // Choose-only tables have no dice; incomplete tables may not have
        // dice yet. Neither needs faces for every result.
        if table.selection == SelectionMode::Choose || (incomplete && table.dice.is_none()) {
            return;
        }
        if table.dice.is_none() {
            self.error(loc, IssueKind::MissingDice);
            return;
        }
        if table.ordered {
            self.check_scale_order(table);
        }

        let layouts = Layout::all(table);
        // A variant that overrides neither dice nor ranges is the base layout
        // again; checking it would only repeat the base layout's issues.
        for layout in layouts.iter().filter(|l| {
            l.variant
                .is_none_or(|v| v.dice.is_some() || v.ranges.is_some())
        }) {
            self.check_layout(table, layout);
        }

        for result in &table.results {
            let reachable = layouts
                .iter()
                .any(|l| l.entries.iter().any(|(id, _)| *id == &result.id));
            if !reachable {
                self.warning(
                    loc.clone().with_result(&result.id),
                    IssueKind::UnreachableResult,
                );
            }
        }
    }

    /// An ordered table's base faces must ascend in result order. Results
    /// that only appear in variants are not on the base layout.
    fn check_scale_order(&mut self, table: &Table) {
        let mut highest: Option<i32> = None;
        for result in &table.results {
            let Some(range) = result.range else { continue };
            if highest.is_some_and(|h| range.min <= h) {
                self.warning(
                    Location::table(&table.id).with_result(&result.id),
                    IssueKind::ScaleOutOfOrder,
                );
            }
            highest = Some(highest.map_or(range.max, |h| h.max(range.max)));
        }
    }

    fn check_follow_ups(&mut self, table: &Table) {
        let mut ids = HashSet::new();
        for follow_up in &table.follow_ups {
            let loc = Location::table(&table.id).with_follow_up(&follow_up.id);
            self.check_id(&follow_up.id, loc.clone());
            if !ids.insert(&follow_up.id) {
                self.error(
                    loc.clone(),
                    IssueKind::DuplicateId {
                        scope: IdScope::FollowUp,
                        id: follow_up.id.clone(),
                    },
                );
            }
            self.check_condition(follow_up.when.as_ref(), &loc);
            let referenced = follow_up
                .on_results
                .iter()
                .chain(follow_up.outcomes.iter().filter_map(|o| o.becomes.as_ref()));
            for result in referenced {
                if table.result(result).is_none() {
                    self.error(
                        loc.clone(),
                        IssueKind::BrokenResultReference {
                            table: table.id.clone(),
                            result: result.clone(),
                        },
                    );
                }
            }
            let entries: Vec<(String, Location, FaceRange)> = follow_up
                .outcomes
                .iter()
                .map(|o| {
                    let label = o
                        .becomes
                        .as_ref()
                        .map_or_else(|| "(keep)".to_owned(), ToString::to_string);
                    (label, loc.clone(), o.range)
                })
                .collect();
            self.check_ranges(&loc, &follow_up.dice, &entries, true);
        }
    }

    fn check_layout(&mut self, table: &Table, layout: &Layout<'_>) {
        let loc = Location::table(&table.id).with_variant(layout.variant.map(|v| &v.id));
        let Some(dice) = layout.dice else {
            return;
        };
        // A variant that inherits the table's dice re-reports nothing new for
        // an invalid expression; only the base layout reports it.
        let report_dice = layout.variant.is_none_or(|v| v.dice.is_some());
        let entries: Vec<(String, Location, FaceRange)> = layout
            .entries
            .iter()
            .map(|(id, range)| (id.to_string(), loc.clone().with_result(id), *range))
            .collect();
        self.check_ranges(&loc, dice, &entries, report_dice);
    }

    /// Checks that `entries` cover every total of `dice` exactly once.
    fn check_ranges(
        &mut self,
        loc: &Location,
        dice: &str,
        entries: &[(String, Location, FaceRange)],
        report_dice: bool,
    ) {
        let expr = match DiceExpr::parse(dice) {
            Ok(expr) => expr,
            Err(error) => {
                if report_dice {
                    self.error(
                        loc.clone(),
                        IssueKind::InvalidDice {
                            expression: dice.to_owned(),
                            error,
                        },
                    );
                }
                return;
            }
        };
        let possible = FaceRange::new(expr.min_total(), expr.max_total());

        let mut valid = Vec::new();
        for (label, rloc, range) in entries {
            let rloc = rloc.clone();
            if range.is_empty() {
                self.error(
                    rloc,
                    IssueKind::ImpossibleRange {
                        range: *range,
                        possible: None,
                    },
                );
            } else if range.min < possible.min || range.max > possible.max {
                self.error(
                    rloc,
                    IssueKind::ImpossibleRange {
                        range: *range,
                        possible: Some(possible),
                    },
                );
            } else {
                valid.push((label, *range));
            }
        }

        valid.sort_by_key(|(_, r)| (r.min, r.max));
        for (i, (first, a)) in valid.iter().enumerate() {
            for (second, b) in &valid[i + 1..] {
                if b.min > a.max {
                    break;
                }
                if let Some(overlap) = a.intersection(b) {
                    self.error(
                        loc.clone(),
                        IssueKind::OverlappingRanges {
                            first: (*first).clone(),
                            second: (*second).clone(),
                            overlap,
                        },
                    );
                }
            }
        }

        let mut outcomes = Vec::new();
        let mut next = possible.min;
        for (_, r) in &valid {
            if r.min > next {
                outcomes.push(FaceRange::new(next, r.min - 1));
            }
            next = next.max(r.max + 1);
        }
        if next <= possible.max {
            outcomes.push(FaceRange::new(next, possible.max));
        }
        if !outcomes.is_empty() {
            self.error(
                loc.clone(),
                IssueKind::UncoveredOutcomes {
                    dice: expr.to_string(),
                    outcomes,
                },
            );
        }
    }

    fn check_tag_effects(&mut self, tag: &crate::content::Tag) {
        for (index, def) in tag.effects.iter().enumerate() {
            if !def.is_active() {
                continue;
            }
            let loc = Location::tag(&tag.id).with_effect(index);
            let table = match def.effect.target_table() {
                Some(id) => match self.tables.get(id) {
                    Some(t) => Some(*t),
                    None => {
                        self.error(loc.clone(), IssueKind::MissingTable { table: id.clone() });
                        continue;
                    }
                },
                None => None,
            };
            match &def.effect {
                Effect::RouteTo {
                    variant: Some(variant),
                    ..
                }
                | Effect::UseVariant { variant, .. } => {
                    let table = table.expect("variant effects target a table");
                    if table.variant(variant).is_none() {
                        self.error(
                            loc,
                            IssueKind::MissingVariant {
                                table: table.id.clone(),
                                variant: variant.clone(),
                            },
                        );
                    }
                }
                Effect::RouteTo { variant: None, .. } => {}
                Effect::RestrictResults { only, exclude, .. } => {
                    let table = table.expect("restrict targets a table");
                    for result in only.iter().flatten().chain(exclude) {
                        if table.result(result).is_none() {
                            self.error(
                                loc.clone(),
                                IssueKind::BrokenResultReference {
                                    table: table.id.clone(),
                                    result: result.clone(),
                                },
                            );
                        }
                    }
                }
                Effect::ModifyDice { dice, .. } => {
                    if let Some(dice) = dice {
                        self.check_dice(dice, &loc);
                    }
                }
                Effect::SecondaryRoll { dice, .. } => self.check_dice(dice, &loc),
                Effect::AddTag { tag } => self.check_tag_ref(tag, &loc),
                Effect::RedirectTable { to, .. } => {
                    if !self.tables.contains_key(to) {
                        self.error(loc, IssueKind::MissingTable { table: to.clone() });
                    }
                }
                Effect::Intent { .. } => self.error(loc, IssueKind::IntentNotPlanned),
            }
        }
    }

    fn check_dice(&mut self, dice: &str, loc: &Location) {
        if let Err(error) = DiceExpr::parse(dice) {
            self.error(
                loc.clone(),
                IssueKind::InvalidDice {
                    expression: dice.to_owned(),
                    error,
                },
            );
        }
    }

    /// Builds the table → table routing graph and reports cycles. An edge
    /// T → U exists if some rollable result of T emits (directly or through
    /// `add_tag`) a tag that routes to U; it is *certain* if every rollable
    /// result of T does.
    fn check_routing_cycles(&mut self) {
        let content = self.content;
        let index: HashMap<&TableId, usize> = content
            .tables
            .iter()
            .enumerate()
            .rev() // first occurrence wins for duplicates
            .map(|(i, t)| (&t.id, i))
            .collect();
        let n = content.tables.len();
        let mut any = vec![BTreeSet::new(); n];
        let mut certain = vec![BTreeSet::new(); n];

        for (i, table) in content.tables.iter().enumerate() {
            let layouts = Layout::all(table);
            // Results of choose-only or dice-less tables are reached by choice.
            let chosen = table.selection == SelectionMode::Choose || table.dice.is_none();
            let rollable: Vec<_> = table
                .results
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| {
                    chosen
                        || layouts
                            .iter()
                            .any(|l| l.entries.iter().any(|(id, _)| *id == &r.id))
                })
                .collect();
            let mut per_result: Vec<BTreeSet<usize>> = Vec::new();
            for result in &rollable {
                let targets: BTreeSet<usize> = route_targets(content, &result.tags)
                    .into_iter()
                    .filter_map(|t| index.get(t).copied())
                    .collect();
                any[i].extend(targets.iter().copied());
                per_result.push(targets);
            }
            if let Some((first, rest)) = per_result.split_first() {
                certain[i] = rest.iter().fold(first.clone(), |acc, s| {
                    acc.intersection(s).copied().collect()
                });
            }
        }

        let guaranteed: Vec<Vec<usize>> = cyclic_components(&certain);
        let covered: HashSet<usize> = guaranteed.iter().flatten().copied().collect();
        for component in &guaranteed {
            self.report_cycle(component, true);
        }
        for component in cyclic_components(&any) {
            if !component.iter().any(|i| covered.contains(i)) {
                self.report_cycle(&component, false);
            }
        }
    }

    fn report_cycle(&mut self, component: &[usize], guaranteed: bool) {
        let tables: Vec<TableId> = component
            .iter()
            .map(|&i| self.content.tables[i].id.clone())
            .collect();
        let loc = Location::table(&tables[0]);
        let kind = IssueKind::RoutingCycle { tables, guaranteed };
        if guaranteed {
            self.error(loc, kind);
        } else {
            self.warning(loc, kind);
        }
    }
}

/// Tables routed to when `tags` are emitted, following `add_tag` chains
/// through enabled tags and active effects.
pub(crate) fn route_targets<'a>(content: &'a ContentSet, tags: &[TagId]) -> Vec<&'a TableId> {
    let mut seen: HashSet<&TagId> = HashSet::new();
    let mut stack: Vec<&TagId> = tags.iter().collect();
    let mut targets = Vec::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(tag) = content.tag(id).filter(|t| t.enabled) else {
            continue;
        };
        for def in tag.effects.iter().filter(|d| d.is_active()) {
            match &def.effect {
                Effect::RouteTo { table, .. } => targets.push(table),
                Effect::AddTag { tag } => stack.push(tag),
                _ => {}
            }
        }
    }
    targets
}

/// Strongly connected components that contain a cycle (size > 1, or a
/// self-loop), each sorted by table index. Tarjan's algorithm.
fn cyclic_components(edges: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    struct State<'e> {
        edges: &'e [BTreeSet<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }

    fn visit(s: &mut State<'_>, v: usize) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on_stack[v] = true;
        for &w in s.edges[v].iter() {
            match s.index[w] {
                None => {
                    visit(s, w);
                    s.low[v] = s.low[v].min(s.low[w]);
                }
                Some(wi) if s.on_stack[w] => s.low[v] = s.low[v].min(wi),
                Some(_) => {}
            }
        }
        if Some(s.low[v]) == s.index[v] {
            let mut component = Vec::new();
            loop {
                let w = s.stack.pop().expect("stack holds component");
                s.on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            if component.len() > 1 || s.edges[v].contains(&v) {
                component.sort_unstable();
                s.out.push(component);
            }
        }
    }

    let n = edges.len();
    let mut s = State {
        edges,
        index: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if s.index[v].is_none() {
            visit(&mut s, v);
        }
    }
    s.out.sort();
    s.out
}
