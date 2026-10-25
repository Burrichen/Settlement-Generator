//! The Dice Assistant: lays a table's results out on real tabletop dice.
//!
//! Given each result's relative chance (its weight; all 1 for equal
//! chances), it finds the physical dice that represent those chances best:
//!
//! - **exact**: one standard die whose faces share out evenly (5 equal
//!   results on 1d10, 2 faces each), or a sum of dice whose bell curve
//!   matches the weights exactly (e.g. 2d6);
//! - **roll again**: a die with a few spare faces that are rolled again,
//!   which is still exact (7 equal results on 1d8, rolling 8 again);
//! - **approximate**: the closest layout on one die (e.g. 1d100), with the
//!   largest difference from the wanted chance stated;
//! - and, when no clean die fits, how many results to add or remove (or what
//!   the weights should add up to) for one that does.
//!
//! Everything is a layout of faces on dice the user can roll at the table:
//! nothing ever falls back to software picking a result. Interfaces show the
//! plain-English explanations as they are.

use serde::{Deserialize, Serialize};

use crate::content::FaceRange;
use crate::dice::DiceExpr;

/// Single dice the assistant considers, smallest first.
pub const STANDARD_DICE: [u32; 7] = [4, 6, 8, 10, 12, 20, 100];

/// Sums of dice whose bell curves the assistant tries for unequal chances.
pub const CURVES: [&str; 6] = ["2d4", "2d6", "2d8", "2d10", "3d6", "1d4+1d6"];

/// More than this share of faces rolled again is not offered.
const MAX_REROLL_SHARE: f64 = 1.0 / 3.0;

/// How a suggested layout represents the wanted chances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitKind {
    /// Every chance is exactly right on one die.
    Exact,
    /// Exactly right on a sum of dice (a bell curve).
    Curve,
    /// Exactly right, with some faces rolled again.
    RollAgain,
    /// Close, but not exactly the wanted chances.
    Approximate,
}

/// One way of laying the results out on dice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiceFit {
    /// The dice to roll, e.g. `1d10`.
    pub dice: String,
    pub kind: FitKind,
    /// Short title, e.g. "1d10, 2 faces each".
    pub label: String,
    /// Plain-English account of the layout.
    pub explanation: String,
    /// The faces of each result, in the order given.
    pub ranges: Vec<FaceRange>,
    /// Faces that are rolled again, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll_again: Option<FaceRange>,
    /// Each result's chance with this layout (0–1), rerolls accounted for.
    pub chances: Vec<f64>,
    /// The largest difference from a wanted chance, in percentage points.
    pub max_error: f64,
}

/// A change to the results that would allow a cleaner die.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryChangeKind {
    Add,
    Remove,
    /// Change the weights so they add up to a different total.
    Reweight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryChange {
    pub kind: EntryChangeKind,
    /// Results to add or remove, or the new total of the weights.
    pub count: u32,
    pub dice: String,
    pub explanation: String,
}

/// The assistant's advice for a set of weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiceAdvice {
    /// The wanted chance of each result (0–1).
    pub targets: Vec<f64>,
    /// The best layout first. Empty only when no dice can hold the results.
    pub suggestions: Vec<DiceFit>,
    /// Plain-English summary, including why no clean die fits when none
    /// does.
    pub summary: String,
    /// Optional changes to the results that would allow a cleaner die.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_changes: Vec<EntryChange>,
}

/// Every possible total of `expr` and how many combinations of the dice
/// give it, lowest first.
pub(crate) fn total_distribution(expr: &DiceExpr) -> Vec<(i32, u64)> {
    combinations(expr)
}

/// Every possible total of `expr` and how many combinations of the dice
/// give it, lowest first.
fn combinations(expr: &DiceExpr) -> Vec<(i32, u64)> {
    let mut dist: Vec<u64> = vec![1];
    for term in expr.terms() {
        let sides = term.die.sides() as usize;
        for _ in 0..term.count {
            let mut next = vec![0u64; dist.len() + sides];
            for (sum, n) in dist.iter().enumerate() {
                for face in 1..=sides {
                    next[sum + face] += n;
                }
            }
            dist = next;
        }
    }
    dist.into_iter()
        .enumerate()
        .filter(|(_, n)| *n > 0)
        .map(|(sum, n)| (sum as i32 + expr.modifier(), n))
        .collect()
}

fn percent(p: f64) -> String {
    let v = p * 100.0;
    if (v - v.round()).abs() < 1e-9 {
        format!("{}%", v.round())
    } else {
        format!("{v:.2}%")
    }
}

fn faces_word(n: u32) -> &'static str {
    if n == 1 { "face" } else { "faces" }
}

fn targets_of(weights: &[u32]) -> Vec<f64> {
    let total: u32 = weights.iter().sum();
    weights
        .iter()
        .map(|w| f64::from(*w) / f64::from(total.max(1)))
        .collect()
}

fn all_equal(weights: &[u32]) -> bool {
    weights.windows(2).all(|w| w[0] == w[1])
}

/// Lays `weights` out on `dice`, keeping the highest `roll_again` totals
/// as faces rolled again. Results take consecutive totals in order, each at
/// least one, split where the running chance comes closest to the wanted
/// one. `None` when the dice have too few totals.
fn layout(weights: &[u32], dice: &str, roll_again: u32) -> Option<DiceFit> {
    let expr = DiceExpr::parse(dice).ok()?;
    let combos = combinations(&expr);
    let usable = combos.len().checked_sub(roll_again as usize)?;
    if weights.is_empty() || usable < weights.len() {
        return None;
    }
    let total_combos: u64 = combos[..usable].iter().map(|(_, n)| n).sum();
    let wanted_total: u64 = weights.iter().map(|w| u64::from(*w)).sum();
    if wanted_total == 0 {
        return None;
    }
    // Cumulative combinations after each total.
    let mut cum = Vec::with_capacity(usable);
    let mut running = 0u64;
    for (_, n) in &combos[..usable] {
        running += n;
        cum.push(running);
    }
    let mut ranges = Vec::with_capacity(weights.len());
    let mut start = 0usize;
    let mut wanted_so_far = 0u64;
    for (i, w) in weights.iter().enumerate() {
        wanted_so_far += u64::from(*w);
        let remaining = weights.len() - i - 1;
        let end = if remaining == 0 {
            usable - 1
        } else {
            // Compare running/total_combos with wanted_so_far/wanted_total
            // without rounding: cross-multiply.
            let goal = |idx: usize| {
                (i128::from(cum[idx]) * i128::from(wanted_total)
                    - i128::from(wanted_so_far) * i128::from(total_combos))
                .abs()
            };
            (start..usable - remaining)
                .min_by_key(|&idx| (goal(idx), idx))
                .expect("at least one total left")
        };
        ranges.push(FaceRange::new(combos[start].0, combos[end].0));
        start = end + 1;
    }
    let chances: Vec<f64> = ranges
        .iter()
        .map(|r| {
            combos
                .iter()
                .filter(|(t, _)| r.contains(*t))
                .map(|(_, n)| *n as f64)
                .sum::<f64>()
                / total_combos as f64
        })
        .collect();
    let exact = ranges.iter().zip(weights).all(|(r, w)| {
        let got: u64 = combos
            .iter()
            .filter(|(t, _)| r.contains(*t))
            .map(|(_, n)| n)
            .sum();
        u128::from(got) * u128::from(wanted_total) == u128::from(*w) * u128::from(total_combos)
    });
    let targets = targets_of(weights);
    let max_error = chances
        .iter()
        .zip(&targets)
        .map(|(c, t)| (c - t).abs() * 100.0)
        .fold(0.0, f64::max);
    let roll_again =
        (roll_again > 0).then(|| FaceRange::new(combos[usable].0, combos[combos.len() - 1].0));
    let single = expr.terms().len() == 1 && expr.terms()[0].count == 1;
    let kind = match (exact, roll_again.is_some(), single) {
        (true, true, _) => FitKind::RollAgain,
        (true, false, true) => FitKind::Exact,
        (true, false, false) => FitKind::Curve,
        (false, _, _) => FitKind::Approximate,
    };
    Some(DiceFit {
        dice: expr.to_string(),
        kind,
        label: String::new(),
        explanation: String::new(),
        ranges,
        roll_again,
        chances,
        max_error: if exact { 0.0 } else { max_error },
    })
}

/// Lays `weights` out on `dice` as closely as possible, without faces
/// rolled again. For the editors' Equal and Custom chance modes, once the
/// user has picked dice.
pub fn fit(weights: &[u32], dice: &str) -> Result<DiceFit, String> {
    let expr = DiceExpr::parse(dice).map_err(|e| e.to_string())?;
    if weights.is_empty() {
        return Err("Add at least one result.".into());
    }
    if weights.contains(&0) {
        return Err("Every chance must be at least 1.".into());
    }
    let mut fit = layout(weights, dice, 0).ok_or_else(|| {
        format!(
            "{expr} has only {} possible totals, fewer than the {} results.",
            combinations(&expr).len(),
            weights.len()
        )
    })?;
    describe(&mut fit, weights);
    Ok(fit)
}

/// How many ways `dice` can roll into each of `ranges` (a result's weight
/// on that layout). `None` for dice that cannot be read.
pub(crate) fn range_weights(ranges: &[FaceRange], dice: &str) -> Option<Vec<u64>> {
    let expr = DiceExpr::parse(dice).ok()?;
    let combos = combinations(&expr);
    Some(
        ranges
            .iter()
            .map(|r| {
                combos
                    .iter()
                    .filter(|(total, _)| r.contains(*total))
                    .map(|(_, n)| n)
                    .sum()
            })
            .collect(),
    )
}

/// Fills in a fit's label and explanation.
fn describe(fit: &mut DiceFit, weights: &[u32]) {
    let equal = all_equal(weights);
    let widths: Vec<i32> = fit.ranges.iter().map(|r| r.max - r.min + 1).collect();
    let even = widths.windows(2).all(|w| w[0] == w[1]);
    let per = widths.first().copied().unwrap_or(0) as u32;
    let reroll = fit.roll_again.map(|r| {
        if r.min == r.max {
            format!("roll again on a {}", r.min)
        } else {
            format!("roll again on {}–{}", r.min, r.max)
        }
    });
    fit.label = match (fit.kind, equal && even, &reroll) {
        (FitKind::Exact, true, _) => format!("{}, {per} {} each", fit.dice, faces_word(per)),
        (FitKind::Exact, false, _) => format!("{}, exact", fit.dice),
        (FitKind::Curve, _, _) => format!("{} (bell curve), exact", fit.dice),
        (FitKind::RollAgain, true, Some(r)) => {
            format!("{}, {per} {} each, {r}", fit.dice, faces_word(per))
        }
        (FitKind::RollAgain, _, Some(r)) => format!("{}, {r}", fit.dice),
        _ => format!("{}, approximate", fit.dice),
    };
    let chances: Vec<String> = fit.chances.iter().map(|c| percent(*c)).collect();
    fit.explanation = match fit.kind {
        FitKind::Exact if equal => format!(
            "Roll {}. Each result has {per} {}, so each has exactly {}.",
            fit.dice,
            faces_word(per),
            chances.first().cloned().unwrap_or_default()
        ),
        FitKind::Exact => format!(
            "Roll {}. Each result has as many faces as its chance, so every chance is exact ({}).",
            fit.dice,
            chances.join(", ")
        ),
        FitKind::Curve => format!(
            "Roll {} and add the dice together. Middle totals come up more often, which matches these chances exactly ({}).",
            fit.dice,
            chances.join(", ")
        ),
        FitKind::RollAgain => format!(
            "Roll {} and {}. Every result then has exactly the chance wanted ({}). Rolling again is a normal tabletop technique, and the app does the same with its own dice.",
            fit.dice,
            reroll.clone().unwrap_or_default(),
            chances.join(", ")
        ),
        FitKind::Approximate => format!(
            "Roll {}. The chances come out as {}, at most {:.2} percentage points from what was asked.",
            fit.dice,
            chances.join(", "),
            fit.max_error
        ),
    };
}

/// Advice for laying out results with these relative chances.
pub fn advise(weights: &[u32]) -> DiceAdvice {
    let targets = targets_of(weights);
    if weights.is_empty() {
        return DiceAdvice {
            targets,
            suggestions: Vec::new(),
            summary: "Add at least one result, then the assistant can suggest dice.".into(),
            entry_changes: Vec::new(),
        };
    }
    if weights.contains(&0) {
        return DiceAdvice {
            targets,
            suggestions: Vec::new(),
            summary: "Every result needs a chance of at least 1.".into(),
            entry_changes: Vec::new(),
        };
    }
    let n = weights.len() as u32;
    let total: u32 = weights.iter().sum();
    let equal = all_equal(weights);
    let mut suggestions: Vec<DiceFit> = Vec::new();
    let push = |mut fit: DiceFit, suggestions: &mut Vec<DiceFit>| {
        describe(&mut fit, weights);
        if !suggestions
            .iter()
            .any(|s| s.dice == fit.dice && s.kind == fit.kind)
        {
            suggestions.push(fit);
        }
    };

    // Exact on one die: the smallest die the total divides.
    for sides in STANDARD_DICE.iter().filter(|s| *s % total == 0) {
        if let Some(fit) = layout(weights, &format!("1d{sides}"), 0) {
            push(fit, &mut suggestions);
        }
    }
    // Exact bell curves, for unequal chances.
    if !equal {
        for curve in CURVES {
            if let Some(fit) = layout(weights, curve, 0).filter(|f| f.kind == FitKind::Curve) {
                push(fit, &mut suggestions);
            }
        }
    }
    let exact = !suggestions.is_empty();
    // Exact with faces rolled again, on the smallest suitable dice.
    if !exact {
        let mut rerolls = 0;
        for sides in STANDARD_DICE.iter().filter(|s| **s > total) {
            let spare = sides % total;
            if f64::from(spare) / f64::from(*sides) > MAX_REROLL_SHARE {
                continue;
            }
            if let Some(fit) = layout(weights, &format!("1d{sides}"), spare) {
                push(fit, &mut suggestions);
                rerolls += 1;
            }
            if rerolls == 2 {
                break;
            }
        }
    }
    // The closest approximations without rerolling.
    for sides in [20, 100] {
        if let Some(fit) = layout(weights, &format!("1d{sides}"), 0)
            .filter(|f| f.kind == FitKind::Approximate && f.max_error < 5.0)
        {
            push(fit, &mut suggestions);
        }
    }
    if !exact {
        // The best exact option first, then approximations by accuracy.
        suggestions.sort_by(|a, b| {
            let rank = |f: &DiceFit| u8::from(f.kind == FitKind::Approximate);
            rank(a)
                .cmp(&rank(b))
                .then(a.max_error.total_cmp(&b.max_error))
        });
    }

    let entry_changes = if exact {
        Vec::new()
    } else {
        entry_changes(weights, equal)
    };
    let summary = summary(equal, n, total, &suggestions);
    DiceAdvice {
        targets,
        suggestions,
        summary,
        entry_changes,
    }
}

/// The smallest standard die that `total` divides, if any.
fn clean_die(total: u32) -> Option<u32> {
    STANDARD_DICE.into_iter().find(|s| s % total == 0)
}

fn entry_changes(weights: &[u32], equal: bool) -> Vec<EntryChange> {
    let n = weights.len() as u32;
    let mut out = Vec::new();
    if equal {
        for delta in 1..=3u32 {
            if let Some(sides) = n.checked_sub(delta).filter(|c| *c > 0).and_then(clean_die) {
                out.push(EntryChange {
                    kind: EntryChangeKind::Remove,
                    count: delta,
                    dice: format!("1d{sides}"),
                    explanation: format!(
                        "Remove {delta} result{} ({} left): then 1d{sides} gives each {} {} exactly.",
                        if delta == 1 { "" } else { "s" },
                        n - delta,
                        sides / (n - delta),
                        faces_word(sides / (n - delta))
                    ),
                });
                break;
            }
        }
        for delta in 1..=3u32 {
            if let Some(sides) = clean_die(n + delta) {
                out.push(EntryChange {
                    kind: EntryChangeKind::Add,
                    count: delta,
                    dice: format!("1d{sides}"),
                    explanation: format!(
                        "Add {delta} result{} ({} in all): then 1d{sides} gives each {} {} exactly.",
                        if delta == 1 { "" } else { "s" },
                        n + delta,
                        sides / (n + delta),
                        faces_word(sides / (n + delta))
                    ),
                });
                break;
            }
        }
    } else {
        let total: u32 = weights.iter().sum();
        let mut totals: Vec<u32> = (total.saturating_sub(3).max(n)..=total + 3)
            .filter(|t| *t != total && clean_die(*t).is_some())
            .collect();
        totals.sort_by_key(|t| (t.abs_diff(total), *t));
        for t in totals.into_iter().take(2) {
            let sides = clean_die(t).expect("filtered");
            out.push(EntryChange {
                kind: EntryChangeKind::Reweight,
                count: t,
                dice: format!("1d{sides}"),
                explanation: format!(
                    "Make the chances add up to {t} instead of {total}: then 1d{sides} gives every result an exact share."
                ),
            });
        }
    }
    out
}

fn summary(equal: bool, n: u32, total: u32, suggestions: &[DiceFit]) -> String {
    let what = if equal {
        format!("{n} equal result{}", if n == 1 { "" } else { "s" })
    } else {
        format!("{n} results whose chances add up to {total}")
    };
    let Some(best) = suggestions.first() else {
        return format!(
            "{what} cannot be laid out on standard dice: even 1d100 has only 100 faces. Split the table into two tables, or combine some results."
        );
    };
    match best.kind {
        FitKind::Exact | FitKind::Curve => {
            format!("{what} fit {} exactly: {}.", best.dice, best.label)
        }
        _ => {
            let dice: Vec<String> = STANDARD_DICE.iter().map(|s| s.to_string()).collect();
            let reason = if equal {
                format!(
                    "{n} does not divide evenly into {} or {} faces",
                    dice[..dice.len() - 1].join(", "),
                    dice[dice.len() - 1]
                )
            } else {
                format!("no standard die has a multiple of {total} faces")
            };
            format!(
                "{what} cannot be shared out exactly on one standard die: {reason}. Best option: {}.",
                best.label
            )
        }
    }
}

/// The relative chance of each range on a single die (its width), when the
/// dice are one die; `None` for sums of dice, where faces are not equally
/// likely.
pub fn weights_of(dice: &str, ranges: &[FaceRange]) -> Option<Vec<u32>> {
    let expr = DiceExpr::parse(dice).ok()?;
    let single = expr.terms().len() == 1 && expr.terms()[0].count == 1;
    single.then(|| {
        ranges
            .iter()
            .map(|r| u32::try_from(r.max - r.min + 1).unwrap_or(0))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(fit: &DiceFit) -> Vec<i32> {
        fit.ranges.iter().map(|r| r.max - r.min + 1).collect()
    }

    #[test]
    fn five_equal_results_use_a_d10_two_faces_each() {
        let advice = advise(&[1; 5]);
        let best = &advice.suggestions[0];
        assert_eq!(best.dice, "1d10");
        assert_eq!(best.kind, FitKind::Exact);
        assert_eq!(widths(best), [2; 5]);
        assert_eq!(best.label, "1d10, 2 faces each");
        assert!(advice.summary.contains("fit 1d10 exactly"));
        assert!(advice.entry_changes.is_empty());
    }

    #[test]
    fn the_smallest_clean_die_comes_first() {
        for (n, dice) in [
            (1, "1d4"),
            (2, "1d4"),
            (3, "1d6"),
            (4, "1d4"),
            (6, "1d6"),
            (12, "1d12"),
            (25, "1d100"),
        ] {
            assert_eq!(advise(&vec![1; n]).suggestions[0].dice, dice, "{n}");
        }
    }

    #[test]
    fn seven_equal_results_explain_and_offer_alternatives() {
        let advice = advise(&[1; 7]);
        assert!(advice.summary.contains("cannot be shared out exactly"));
        assert!(advice.summary.contains("7 does not divide evenly"));
        let best = &advice.suggestions[0];
        assert_eq!(best.kind, FitKind::RollAgain);
        assert_eq!(best.dice, "1d8");
        assert_eq!(best.roll_again, Some(FaceRange::single(8)));
        assert!(best.chances.iter().all(|c| (c - 1.0 / 7.0).abs() < 1e-12));
        assert!(
            advice
                .suggestions
                .iter()
                .any(|s| s.kind == FitKind::Approximate && s.dice == "1d100")
        );
        let kinds: Vec<_> = advice
            .entry_changes
            .iter()
            .map(|c| (c.kind.clone(), c.dice.clone()))
            .collect();
        assert_eq!(
            kinds,
            [
                (EntryChangeKind::Remove, "1d6".into()),
                (EntryChangeKind::Add, "1d8".into())
            ]
        );
    }

    #[test]
    fn custom_weights_fit_exactly_when_they_can() {
        // 1 : 2 : 1 shares out exactly on a d4.
        let advice = advise(&[1, 2, 1]);
        assert_eq!(advice.suggestions[0].dice, "1d4");
        assert_eq!(widths(&advice.suggestions[0]), [1, 2, 1]);
        // 2d6 totals: 2-6 = 15, 7 = 6, 8-12 = 15 combinations.
        let advice = advise(&[15, 6, 15]);
        let curve = advice
            .suggestions
            .iter()
            .find(|s| s.kind == FitKind::Curve)
            .unwrap();
        assert_eq!(curve.dice, "2d6");
        assert_eq!(
            curve.ranges,
            [
                FaceRange::new(2, 6),
                FaceRange::single(7),
                FaceRange::new(8, 12)
            ]
        );
    }

    #[test]
    fn fit_lays_weights_out_on_chosen_dice() {
        let fit = fit(&[3, 1], "1d4").unwrap();
        assert_eq!(fit.ranges, [FaceRange::new(1, 3), FaceRange::single(4)]);
        assert_eq!(fit.kind, FitKind::Exact);
        let fit = super::fit(&[1, 1, 1], "1d4").unwrap();
        assert_eq!(fit.kind, FitKind::Approximate);
        assert!(fit.max_error > 0.0);
        assert!(super::fit(&[1; 5], "1d4").is_err());
    }

    #[test]
    fn more_results_than_any_die_has_faces_is_explained() {
        let advice = advise(&[1; 101]);
        assert!(advice.suggestions.is_empty());
        assert!(advice.summary.contains("even 1d100 has only 100 faces"));
    }

    #[test]
    fn weights_come_from_range_widths_on_one_die() {
        let ranges = [FaceRange::new(1, 2), FaceRange::new(3, 6)];
        assert_eq!(weights_of("1d6", &ranges), Some(vec![2, 4]));
        assert_eq!(weights_of("2d6", &ranges), None);
    }
}
