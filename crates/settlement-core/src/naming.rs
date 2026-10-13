//! Settlement naming: data-driven naming schemes, and the deterministic rules
//! that join rolled name parts into a name.
//!
//! A naming scheme applies while its `when` condition holds over the tags
//! generated so far (e.g. Kurovia and Ordon). It names a **method table**:
//! an ordinary table whose results are the scheme's naming methods, so each
//! method's ID, name, description, physical dice, weight (its faces) and
//! optional tag eligibility are edited like any other table. Each method
//! lists **component tables** (also ordinary tables) that are rolled in
//! order, and a **join rule** with explicit spelling exceptions that turns
//! the rolled parts into one name. Nothing is rerolled to improve a name.
//! See `docs/NAMING_SYSTEM.md`.

use serde::{Deserialize, Serialize};

use crate::content::{Condition, ContentSet};
use crate::dice::FaceSource;
use crate::engine::{
    GenerationError, GenerationEvent, Generator, Named, Request, Selections, State, TableRollRecord,
};
use crate::ids::{NamingSchemeId, ResultId, TableId, TagId};

/// The `stage_table` recorded for the name stage in saved snapshots. The
/// name stage has no table of its own; this reserved ID marks it.
pub const NAME_STAGE_ID: &str = "stage.name";

/// What a naming scheme names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamingSubject {
    /// The settlement (the Name stage).
    #[default]
    Settlement,
    /// A person, such as an Authority Figure. Person schemes are chosen by
    /// the person's own tags (e.g. species and gender) as well as the
    /// settlement's.
    Person,
}

impl NamingSubject {
    fn is_settlement(&self) -> bool {
        *self == NamingSubject::Settlement
    }
}

/// A regional (or other) way of naming settlements, or people.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamingScheme {
    pub id: NamingSchemeId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What the scheme names; settlements unless stated.
    #[serde(default, skip_serializing_if = "NamingSubject::is_settlement")]
    pub subject: NamingSubject,
    /// The scheme applies while this holds. The first scheme that applies is
    /// used.
    pub when: Condition,
    /// Rolled (or chosen) to pick the method; its results are the methods.
    pub method_table: TableId,
    pub methods: Vec<NamingMethod>,
}

impl NamingScheme {
    /// The method for a result of the method table.
    pub fn method(&self, result: &ResultId) -> Option<&NamingMethod> {
        self.methods.iter().find(|m| &m.result == result)
    }
}

/// One way of building a name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamingMethod {
    /// The method table's result for this method. Its name, description,
    /// faces (weight) and eligibility are the method's.
    pub result: ResultId,
    /// Tables rolled in order; their results are the parts of the name.
    pub components: Vec<TableId>,
    #[serde(default)]
    pub join: JoinRule,
    /// Exact spellings for particular combinations of component results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<SpellingException>,
    /// What each component is (e.g. given name, family name), in component
    /// order. Optional; when present there is one per component. Lets a
    /// generated person's name record which result is which, and lets a
    /// part be chosen on its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<NamePartDef>,
}

/// What one component of a naming method is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePartDef {
    /// Stable part ID, e.g. `given` or `family`.
    pub id: String,
    /// Display label, e.g. "Given name".
    pub label: String,
}

/// How rolled parts are joined: the parts are written in order, with
/// `separator` between them (and, for `possessive`, a suffix after a part),
/// and the result is capitalised by `case`. Every setting is per method, so
/// each region chooses its own joining. All optional fields default to the
/// original behaviour: no separator, sentence case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRule {
    #[serde(flatten)]
    pub style: JoinStyle,
    /// Written between parts, e.g. `" "` for two-word names.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub separator: String,
    #[serde(default, skip_serializing_if = "Capitalisation::is_sentence")]
    pub case: Capitalisation,
    /// Exact text substitutions applied, in order, to the joined name before
    /// capitalisation (e.g. a phonetic rule such as "aa" → "a"). Matching
    /// ignores case. None by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacements: Vec<Replacement>,
    /// An editor's note on why the method joins as it does. No effect.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// One exact substitution in a joined name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub from: String,
    pub to: String,
}

/// Replaces every occurrence of `from` in `text`, ignoring case.
fn replace_ignoring_case(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_owned();
    }
    let lower = text.to_lowercase();
    let needle = from.to_lowercase();
    // Lower-casing can change byte lengths for some letters; only use the
    // index mapping when it is safe, otherwise match exactly.
    if lower.len() != text.len() {
        return text.replace(from, to);
    }
    let mut out = String::new();
    let mut last = 0;
    for (start, _) in lower.match_indices(&needle) {
        if start < last {
            continue;
        }
        out.push_str(&text[last..start]);
        out.push_str(to);
        last = start + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "style", rename_all = "snake_case")]
pub enum JoinStyle {
    /// Parts joined as they are: Alder + wick → Alderwick; with a space
    /// separator, Khar + Well → Khar Well.
    #[default]
    Compound,
    /// A possessive `suffix` is written after a part when the next part's
    /// result is listed in `before` (or before every next part when `before`
    /// is absent).
    ///
    /// When the part already ends in "s" (ignoring case), `after_s` is
    /// written instead, if given. Without `after_s`, a part that already
    /// ends with `suffix` gets nothing added.
    ///
    /// Examples:
    /// - Godwin + bury → Godwinsbury (suffix `s`), Nerys + bury → Nerysbury;
    /// - Varesh + Well → Varesh's Well (suffix `'s`, after_s `'`);
    /// - Fergus + Point → Fergus's Point (suffix `'s`, no after_s).
    Possessive {
        suffix: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<Vec<ResultId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_s: Option<String>,
    },
    /// Parts joined by a single space, each word capitalised: Khar + Well →
    /// Khar Well. The same as `compound` with `separator: " "` and
    /// `case: "words"`; an explicit `separator` or `case` still applies.
    Space,
    /// The name is chosen, not built: the source part's result holds a map
    /// of individually approved names (`source_field`, normally
    /// `approved_names`), keyed by the result ID of the variant part. The
    /// approved name is used exactly as written: no suffix, possessive,
    /// separator, capitalisation or replacement is applied, and a missing
    /// name is an error (never a reroll or a fallback join).
    ///
    /// Founder Oswald (`approved_names: { "a": "Oswalden", "b": "Oswaldwick" }`)
    /// + variant `b` → Oswaldwick.
    CuratedVariant {
        /// The result field holding the approved names.
        #[serde(default = "default_source_field")]
        source_field: String,
        /// Zero-based index of the component whose result holds the names.
        #[serde(default, skip_serializing_if = "is_zero")]
        source_from_result_id: usize,
        /// Zero-based index of the component whose result ID selects the
        /// name.
        #[serde(default = "default_variant_index")]
        variant_from_result_id: usize,
        #[serde(default)]
        on_missing: OnMissing,
    },
}

/// The result field `curated_variant` reads unless another is named. The
/// only field results store approved names in.
pub const APPROVED_NAMES_FIELD: &str = "approved_names";

fn default_source_field() -> String {
    APPROVED_NAMES_FIELD.to_owned()
}

fn default_variant_index() -> usize {
    1
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What `curated_variant` does when a source result has no approved name
/// for the variant rolled. Only `error` exists: the name is never improvised.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnMissing {
    #[default]
    Error,
}

impl JoinStyle {
    /// The style's name as written in content, e.g. `curated_variant`.
    pub fn key(&self) -> &'static str {
        match self {
            JoinStyle::Compound => "compound",
            JoinStyle::Possessive { .. } => "possessive",
            JoinStyle::Space => "space",
            JoinStyle::CuratedVariant { .. } => "curated_variant",
        }
    }
}

/// How a joined name is capitalised.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capitalisation {
    /// First letter upper case, every other letter lower case: Alderwick.
    #[default]
    Sentence,
    /// Each space-separated word: first letter upper case, the rest lower
    /// case: Khar Well, Varesh's Well.
    Words,
    /// Exactly as the parts are written.
    AsWritten,
}

impl Capitalisation {
    fn is_sentence(&self) -> bool {
        matches!(self, Capitalisation::Sentence)
    }

    fn apply(self, text: &str) -> String {
        match self {
            Capitalisation::Sentence => capitalise(text),
            Capitalisation::Words => text
                .split(' ')
                .map(capitalise)
                .collect::<Vec<_>>()
                .join(" "),
            Capitalisation::AsWritten => text.to_owned(),
        }
    }
}

/// When the component results are exactly `results` (in order), the name is
/// `name`, written as given.
///
/// `results` lists stable result IDs, so renaming a component never breaks
/// an exception. For content written by name (`"components": ["Ash",
/// "ham"], "output": "Asham"`), `components` lists the component results'
/// display names instead; validation requires each to name exactly one
/// result. Use one form or the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellingException {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<ResultId>,
    /// Display names of the component results, in order (the by-name form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(alias = "output")]
    pub name: String,
    /// Why the exception exists. No effect.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl SpellingException {
    /// How many components the exception lists, in whichever form.
    pub fn len(&self) -> usize {
        self.results.len().max(self.components.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn matches(&self, parts: &[NamePart<'_>]) -> bool {
        if !self.results.is_empty() {
            self.results.len() == parts.len()
                && self.results.iter().zip(parts).all(|(r, p)| r == p.result)
        } else {
            !self.components.is_empty()
                && self.components.len() == parts.len()
                && self
                    .components
                    .iter()
                    .zip(parts)
                    .all(|(c, p)| c.trim() == p.text.trim())
        }
    }
}

/// One rolled part of a name: the table it came from, the component
/// result and its text.
#[derive(Debug, Clone, Copy)]
pub struct NamePart<'a> {
    pub table: &'a TableId,
    pub result: &'a ResultId,
    pub text: &'a str,
}

impl<'a> NamePart<'a> {
    /// The part a roll record gave: the table actually used, its result and
    /// the result's display name.
    pub fn of(record: &'a TableRollRecord) -> Self {
        NamePart {
            table: &record.table.id,
            result: &record.result.id,
            text: &record.result.name,
        }
    }
}

/// How a name was put together from its parts, saved with the name so the
/// trace shows exactly which rule produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameAssembly {
    /// The join style used, e.g. `compound` or `curated_variant`.
    pub join: String,
    /// The spelling exception used instead of the join, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception: Option<SpellingException>,
    /// For `curated_variant`: which approved name was read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curated: Option<CuratedSelection>,
}

/// The approved name a `curated_variant` join read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuratedSelection {
    /// The source table and result holding the approved names.
    pub table: TableId,
    pub source: ResultId,
    pub field: String,
    /// The variant result whose ID selected the name.
    pub variant: ResultId,
}

/// A name and how it was assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedName {
    pub text: String,
    pub assembly: NameAssembly,
}

/// Why parts could not be joined into a name. Validation prevents each of
/// these for content it has accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComposeError {
    #[error(
        "no approved name: result `{source_result}` of table `{table}` has no `{field}` entry for variant `{variant}`"
    )]
    MissingApprovedName {
        table: TableId,
        source_result: ResultId,
        field: String,
        variant: ResultId,
    },
    #[error(
        "curated_variant reads component {index}, but the method has {components} component(s)"
    )]
    MissingComponent { index: usize, components: usize },
    #[error("curated_variant can only read `approved_names`, not `{field}`")]
    UnsupportedField { field: String },
    #[error("table `{table}` has no result `{result}`")]
    UnknownResult { table: TableId, result: ResultId },
}

impl NamingMethod {
    /// Joins `parts` (one per component, in order) into a name. Spelling
    /// exceptions are checked first and used verbatim; otherwise the join
    /// rule applies. A method with one component gives that part exactly as
    /// written. `content` supplies the approved names a `curated_variant`
    /// join reads. Deterministic: the same parts always give the same name,
    /// and nothing is ever rerolled or improvised.
    pub fn compose(
        &self,
        content: &ContentSet,
        parts: &[NamePart<'_>],
    ) -> Result<ComposedName, ComposeError> {
        let assembly = |exception, curated| NameAssembly {
            join: self.join.style.key().to_owned(),
            exception,
            curated,
        };
        if let Some(exception) = self.exceptions.iter().find(|e| e.matches(parts)) {
            return Ok(ComposedName {
                text: exception.name.clone(),
                assembly: assembly(Some(exception.clone()), None),
            });
        }
        if let JoinStyle::CuratedVariant {
            source_field,
            source_from_result_id,
            variant_from_result_id,
            ..
        } = &self.join.style
        {
            let part = |index: usize| {
                parts.get(index).ok_or(ComposeError::MissingComponent {
                    index,
                    components: parts.len(),
                })
            };
            let (source, variant) = (
                part(*source_from_result_id)?,
                part(*variant_from_result_id)?,
            );
            if source_field != APPROVED_NAMES_FIELD {
                return Err(ComposeError::UnsupportedField {
                    field: source_field.clone(),
                });
            }
            let result = content
                .table(source.table)
                .and_then(|t| t.result(source.result))
                .ok_or_else(|| ComposeError::UnknownResult {
                    table: source.table.clone(),
                    result: source.result.clone(),
                })?;
            let text = result
                .approved_names
                .get(variant.result)
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
                .ok_or_else(|| ComposeError::MissingApprovedName {
                    table: source.table.clone(),
                    source_result: source.result.clone(),
                    field: source_field.clone(),
                    variant: variant.result.clone(),
                })?;
            return Ok(ComposedName {
                text: text.to_owned(),
                assembly: assembly(
                    None,
                    Some(CuratedSelection {
                        table: source.table.clone(),
                        source: source.result.clone(),
                        field: source_field.clone(),
                        variant: variant.result.clone(),
                    }),
                ),
            });
        }
        if let [only] = parts {
            return Ok(ComposedName {
                text: only.text.trim().to_owned(),
                assembly: assembly(None, None),
            });
        }
        let (separator, case) = match self.join.style {
            JoinStyle::Space => (
                if self.join.separator.is_empty() {
                    " "
                } else {
                    self.join.separator.as_str()
                },
                if self.join.case.is_sentence() {
                    Capitalisation::Words
                } else {
                    self.join.case
                },
            ),
            _ => (self.join.separator.as_str(), self.join.case),
        };
        let mut joined = String::new();
        for (i, part) in parts.iter().enumerate() {
            joined.push_str(part.text.trim());
            let Some(next) = parts.get(i + 1) else { break };
            if let JoinStyle::Possessive {
                suffix,
                before,
                after_s,
            } = &self.join.style
                && before.as_ref().is_none_or(|b| b.contains(next.result))
            {
                let lower = joined.to_lowercase();
                match after_s {
                    Some(after_s) if lower.ends_with('s') => joined.push_str(after_s),
                    Some(_) => joined.push_str(suffix),
                    None if !lower.ends_with(&suffix.to_lowercase()) => joined.push_str(suffix),
                    None => {}
                }
            }
            joined.push_str(separator);
        }
        for r in &self.join.replacements {
            joined = replace_ignoring_case(&joined, &r.from, &r.to);
        }
        Ok(ComposedName {
            text: case.apply(&joined),
            assembly: assembly(None, None),
        })
    }
}

/// First letter upper case, the rest lower case.
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => String::new(),
    }
}

/// The settlement naming scheme that applies under `tags`: the first whose
/// condition holds.
pub fn scheme_for<'c>(content: &'c ContentSet, tags: &[TagId]) -> Option<&'c NamingScheme> {
    scheme_for_subject(content, NamingSubject::Settlement, tags)
}

/// The first scheme naming `subject` whose condition holds under `tags`.
pub fn scheme_for_subject<'c>(
    content: &'c ContentSet,
    subject: NamingSubject,
    tags: &[TagId],
) -> Option<&'c NamingScheme> {
    content
        .naming_schemes
        .iter()
        .filter(|s| s.subject == subject)
        .find(|s| s.when.evaluate(&|t| tags.contains(t)))
}

/// A name generated by a scheme: its text and method. The rolls are the
/// events the generation added to the state.
pub(crate) struct GeneratedName {
    pub text: String,
    pub method: Named<ResultId>,
    /// The method roll, then one record per component, in order.
    pub rolls: Vec<TableRollRecord>,
    pub assembly: NameAssembly,
}

/// The method a scheme always uses when its method table has no dice and
/// exactly one result can be taken: no roll is needed to pick it.
pub(crate) fn sole_method(
    generator: &Generator<'_>,
    scheme: &NamingScheme,
    state: &State,
) -> Result<Option<ResultId>, GenerationError> {
    if generator
        .context(&scheme.method_table, state, None)?
        .dice
        .is_some()
    {
        return Ok(None);
    }
    let (_, options) = generator.options_for(&scheme.method_table, state)?;
    let mut legal = options.into_iter().filter(|o| o.legal);
    Ok(match (legal.next(), legal.next()) {
        (Some(only), None) => Some(only.result.id),
        _ => None,
    })
}

/// Generates a name with `scheme` under the tags in `state`: rolls (or uses
/// the chosen or sole) method, rolls each component in order with physical dice,
/// and joins the parts. Every roll is recorded in `state`.
pub(crate) fn generate_name(
    generator: &Generator<'_>,
    scheme: &NamingScheme,
    state: &mut State,
    method: Option<ResultId>,
    faces: &mut dyn FaceSource,
) -> Result<GeneratedName, GenerationError> {
    let unavailable = |table: &TableId| GenerationError::NamingIncomplete {
        table: table.clone(),
    };
    let method = match method {
        Some(method) => Some(method),
        None => sole_method(generator, scheme, state)?,
    };
    let start = state.events_so_far().len();
    generator.process(
        state,
        [Request::new(scheme.method_table.clone()).with_choice(method)].into(),
        &Selections::new(),
        faces,
    )?;
    let method_record = match state.events_so_far().get(start) {
        Some(GenerationEvent::TableRolled(r)) => r.clone(),
        _ => return Err(unavailable(&scheme.method_table)),
    };
    let method = scheme
        .method(&method_record.result.id)
        .ok_or_else(|| unavailable(&scheme.method_table))?;
    let mut rolls = vec![(*method_record).clone()];
    for component in &method.components {
        let start = state.events_so_far().len();
        generator.process(
            state,
            [Request::new(component.clone())].into(),
            &Selections::new(),
            faces,
        )?;
        let record = state.events_so_far()[start..]
            .iter()
            .find_map(|e| match e {
                GenerationEvent::TableRolled(r) if r.routed_by.is_none() => Some(r),
                _ => None,
            })
            .ok_or_else(|| unavailable(component))?;
        rolls.push((**record).clone());
    }
    let ComposedName { text, assembly } = method.compose(
        generator.content(),
        &rolls[1..].iter().map(NamePart::of).collect::<Vec<_>>(),
    )?;
    Ok(GeneratedName {
        text,
        method: method_record.result.clone(),
        rolls,
        assembly,
    })
}

/// Whether a name was generated by a scheme or typed by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    Generated,
    Custom,
}

/// A settlement name as saved: the final text, how it came about, and (for
/// generated names) the scheme, method and every roll that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameRecord {
    pub name: String,
    pub source: NameSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<Named<NamingSchemeId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<Named<ResultId>>,
    /// The method roll, then one roll per component, with every die.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rolls: Vec<TableRollRecord>,
    /// How the parts were joined: the join style, and any spelling
    /// exception or approved name used. Absent for typed names and for
    /// names saved before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<NameAssembly>,
    /// Application version that produced this name.
    pub app_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    static TABLE: std::sync::LazyLock<TableId> = std::sync::LazyLock::new(|| "t".into());

    fn part<'a>(result: &'a ResultId, text: &'a str) -> NamePart<'a> {
        NamePart {
            table: &TABLE,
            result,
            text,
        }
    }

    /// The name `m` gives `parts` (no curated names are involved).
    fn join(m: &NamingMethod, parts: &[NamePart<'_>]) -> String {
        m.compose(&ContentSet::default(), parts).unwrap().text
    }

    fn possessive() -> NamingMethod {
        NamingMethod {
            result: "founder".into(),
            components: vec![],
            join: JoinRule {
                style: JoinStyle::Possessive {
                    suffix: "s".into(),
                    before: Some(vec!["bury".into(), "stead".into(), "worth".into()]),
                    after_s: None,
                },
                ..Default::default()
            },
            exceptions: vec![],
            parts: vec![],
        }
    }

    #[test]
    fn compound_joins_and_capitalises() {
        let m = NamingMethod {
            result: "geo".into(),
            components: vec![],
            join: JoinRule::default(),
            exceptions: vec![SpellingException {
                results: vec!["ash".into(), "ham".into()],
                components: vec![],
                name: "Asham".into(),
                note: String::new(),
            }],
            parts: vec![],
        };
        let (alder, wick, ash, ham) = (
            ResultId::from("alder"),
            ResultId::from("wick"),
            ResultId::from("ash"),
            ResultId::from("ham"),
        );
        assert_eq!(
            join(&m, &[part(&alder, "Alder"), part(&wick, "wick")]),
            "Alderwick"
        );
        assert_eq!(
            join(&m, &[part(&alder, "ALDER"), part(&wick, "Wick")]),
            "Alderwick"
        );
        assert_eq!(join(&m, &[part(&ash, "Ash"), part(&ham, "ham")]), "Asham");
        assert_eq!(join(&m, &[part(&ham, "ham"), part(&ash, "Ash")]), "Hamash");
    }

    #[test]
    fn possessive_adds_s_only_before_listed_endings_and_never_twice() {
        let m = possessive();
        let ids: Vec<ResultId> = ["godwin", "nerys", "beric", "bury", "stead", "wick"]
            .map(ResultId::from)
            .to_vec();
        let [godwin, nerys, beric, bury, stead, wick] = &ids[..] else {
            unreachable!()
        };
        assert_eq!(
            join(&m, &[part(godwin, "Godwin"), part(bury, "bury")]),
            "Godwinsbury"
        );
        assert_eq!(
            join(&m, &[part(nerys, "Nerys"), part(bury, "bury")]),
            "Nerysbury"
        );
        assert_eq!(
            join(&m, &[part(beric, "Beric"), part(wick, "wick")]),
            "Bericwick"
        );
        assert_eq!(
            join(&m, &[part(godwin, "Edran"), part(stead, "stead")]),
            "Edransstead"
        );
    }

    fn method(join: serde_json::Value) -> NamingMethod {
        serde_json::from_value(serde_json::json!({
            "result": "m", "components": [], "join": join
        }))
        .unwrap()
    }

    #[test]
    fn a_separator_and_word_case_give_two_word_names() {
        let m =
            method(serde_json::json!({ "style": "compound", "separator": " ", "case": "words" }));
        let (a, b) = (ResultId::from("khar"), ResultId::from("well"));
        assert_eq!(join(&m, &[part(&a, "Khar"), part(&b, "Well")]), "Khar Well");
        assert_eq!(join(&m, &[part(&a, "khar"), part(&b, "WELL")]), "Khar Well");
    }

    #[test]
    fn apostrophe_possessives_with_and_without_an_after_s_form() {
        let (f, w) = (ResultId::from("f"), ResultId::from("w"));
        // Halingo style: an apostrophe alone after a final s.
        let halingo = method(serde_json::json!({
            "style": "possessive", "suffix": "'s", "after_s": "'",
            "separator": " ", "case": "words"
        }));
        assert_eq!(
            join(&halingo, &[part(&f, "Varesh"), part(&w, "Well")]),
            "Varesh's Well"
        );
        assert_eq!(
            join(&halingo, &[part(&f, "Tamas"), part(&w, "Well")]),
            "Tamas' Well"
        );
        // Skgurr style: always 's.
        let skgurr = method(serde_json::json!({
            "style": "possessive", "suffix": "'s", "separator": " ", "case": "words"
        }));
        assert_eq!(
            join(&skgurr, &[part(&f, "Fergus"), part(&w, "Point")]),
            "Fergus's Point"
        );
    }

    #[test]
    fn the_original_join_format_reads_and_writes_unchanged() {
        let json = serde_json::json!({
            "style": "possessive", "suffix": "s", "before": ["bury", "stead", "worth"]
        });
        let rule: JoinRule = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&rule).unwrap(), json);
        let compound: JoinRule =
            serde_json::from_value(serde_json::json!({ "style": "compound" })).unwrap();
        assert_eq!(compound, JoinRule::default());
    }

    #[test]
    fn replacements_apply_in_order_before_capitalisation() {
        let m = method(serde_json::json!({
            "style": "compound",
            "replacements": [ { "from": "aa", "to": "a" }, { "from": "qm", "to": "km" } ]
        }));
        let (a, b) = (ResultId::from("a"), ResultId::from("b"));
        assert_eq!(
            join(&m, &[part(&a, "Mara"), part(&b, "ra")]),
            "Marara",
            "no aa: unchanged"
        );
        assert_eq!(join(&m, &[part(&a, "Luma"), part(&b, "ari")]), "Lumari");
        assert_eq!(join(&m, &[part(&a, "Saq"), part(&b, "mar")]), "Sakmar");
        // Without replacements nothing changes.
        let plain = method(serde_json::json!({ "style": "compound" }));
        assert_eq!(
            join(&plain, &[part(&a, "Luma"), part(&b, "ari")]),
            "Lumaari"
        );
    }
}
