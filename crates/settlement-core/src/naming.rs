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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellingException {
    pub results: Vec<ResultId>,
    pub name: String,
}

/// One rolled part of a name: the component result and its text.
#[derive(Debug, Clone, Copy)]
pub struct NamePart<'a> {
    pub result: &'a ResultId,
    pub text: &'a str,
}

impl NamingMethod {
    /// Joins `parts` (one per component, in order) into a name. Exceptions
    /// are checked first and used verbatim; otherwise the join rule applies.
    /// Deterministic: the same parts always give the same name.
    pub fn compose(&self, parts: &[NamePart<'_>]) -> String {
        if let Some(exception) = self.exceptions.iter().find(|e| {
            e.results.len() == parts.len()
                && e.results.iter().zip(parts).all(|(r, p)| r == p.result)
        }) {
            return exception.name.clone();
        }
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
            joined.push_str(&self.join.separator);
        }
        for r in &self.join.replacements {
            joined = replace_ignoring_case(&joined, &r.from, &r.to);
        }
        self.join.case.apply(&joined)
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
    let mut parts: Vec<(ResultId, String)> = Vec::new();
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
        parts.push((record.result.id.clone(), record.result.name.clone()));
        rolls.push((**record).clone());
    }
    let text = method.compose(
        &parts
            .iter()
            .map(|(result, text)| NamePart { result, text })
            .collect::<Vec<_>>(),
    );
    Ok(GeneratedName {
        text,
        method: method_record.result.clone(),
        rolls,
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
    /// Application version that produced this name.
    pub app_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part<'a>(result: &'a ResultId, text: &'a str) -> NamePart<'a> {
        NamePart { result, text }
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
                name: "Asham".into(),
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
            m.compose(&[part(&alder, "Alder"), part(&wick, "wick")]),
            "Alderwick"
        );
        assert_eq!(
            m.compose(&[part(&alder, "ALDER"), part(&wick, "Wick")]),
            "Alderwick"
        );
        assert_eq!(m.compose(&[part(&ash, "Ash"), part(&ham, "ham")]), "Asham");
        assert_eq!(m.compose(&[part(&ham, "ham"), part(&ash, "Ash")]), "Hamash");
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
            m.compose(&[part(godwin, "Godwin"), part(bury, "bury")]),
            "Godwinsbury"
        );
        assert_eq!(
            m.compose(&[part(nerys, "Nerys"), part(bury, "bury")]),
            "Nerysbury"
        );
        assert_eq!(
            m.compose(&[part(beric, "Beric"), part(wick, "wick")]),
            "Bericwick"
        );
        assert_eq!(
            m.compose(&[part(godwin, "Edran"), part(stead, "stead")]),
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
        assert_eq!(
            m.compose(&[part(&a, "Khar"), part(&b, "Well")]),
            "Khar Well"
        );
        assert_eq!(
            m.compose(&[part(&a, "khar"), part(&b, "WELL")]),
            "Khar Well"
        );
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
            halingo.compose(&[part(&f, "Varesh"), part(&w, "Well")]),
            "Varesh's Well"
        );
        assert_eq!(
            halingo.compose(&[part(&f, "Tamas"), part(&w, "Well")]),
            "Tamas' Well"
        );
        // Skgurr style: always 's.
        let skgurr = method(serde_json::json!({
            "style": "possessive", "suffix": "'s", "separator": " ", "case": "words"
        }));
        assert_eq!(
            skgurr.compose(&[part(&f, "Fergus"), part(&w, "Point")]),
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
            m.compose(&[part(&a, "Mara"), part(&b, "ra")]),
            "Marara",
            "no aa: unchanged"
        );
        assert_eq!(m.compose(&[part(&a, "Luma"), part(&b, "ari")]), "Lumari");
        assert_eq!(m.compose(&[part(&a, "Saq"), part(&b, "mar")]), "Sakmar");
        // Without replacements nothing changes.
        let plain = method(serde_json::json!({ "style": "compound" }));
        assert_eq!(
            plain.compose(&[part(&a, "Luma"), part(&b, "ari")]),
            "Lumaari"
        );
    }
}
