//! Physical dice engine.
//!
//! Every roll is a simulation of real tabletop dice: an expression such as
//! `2d6` is parsed into its individual dice, each die is rolled separately and
//! the faces are summed. Nothing ever substitutes a uniform pick over the total
//! range, and every individual face is recorded so a roll can be checked
//! against, or replaced by, a physical one.

use std::collections::VecDeque;
use std::fmt;
use std::str::FromStr;

use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Upper bound on the number of dice in a single expression.
pub const MAX_DICE: u32 = 20;
/// Upper bound on the absolute value of a constant modifier.
pub const MAX_MODIFIER: i32 = 1000;

/// A physical die.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Die {
    D4,
    D6,
    D8,
    D10,
    D12,
    D20,
    /// Percentile: two d10s read as tens and units; `00` + `0` is 100.
    D100,
}

impl Die {
    pub const ALL: [Die; 7] = [
        Die::D4,
        Die::D6,
        Die::D8,
        Die::D10,
        Die::D12,
        Die::D20,
        Die::D100,
    ];

    pub fn sides(self) -> u32 {
        match self {
            Die::D4 => 4,
            Die::D6 => 6,
            Die::D8 => 8,
            Die::D10 => 10,
            Die::D12 => 12,
            Die::D20 => 20,
            Die::D100 => 100,
        }
    }

    pub fn from_sides(sides: u32) -> Option<Die> {
        Die::ALL.into_iter().find(|d| d.sides() == sides)
    }
}

impl fmt::Display for Die {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", self.sides())
    }
}

/// `count` dice of one kind, e.g. the `2d6` in `2d6+1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiceTerm {
    pub count: u32,
    pub die: Die,
}

/// A validated dice expression: one or more dice terms plus an optional
/// constant modifier.
///
/// Grammar: `term ("+" term)* (("+" | "-") constant)*` where
/// `term = count? "d" sides`. Whitespace is ignored and `D` is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiceExpr {
    terms: Vec<DiceTerm>,
    modifier: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum DiceParseError {
    #[error("dice expression is empty")]
    Empty,
    #[error("`{token}` is not a valid dice term or number")]
    InvalidToken { token: String },
    #[error("d{sides} is not a supported die (use d4, d6, d8, d10, d12, d20 or d100)")]
    UnsupportedDie { sides: u32 },
    #[error("dice count must be at least 1")]
    ZeroCount,
    #[error("dice cannot be subtracted")]
    NegativeDice,
    #[error("expression must contain at least one die")]
    NoDice,
    #[error("expression uses more than {MAX_DICE} dice")]
    TooManyDice,
    #[error("modifier must be between -{MAX_MODIFIER} and {MAX_MODIFIER}")]
    ModifierOutOfRange,
}

impl DiceExpr {
    pub fn parse(input: &str) -> Result<Self, DiceParseError> {
        let compact: String = input
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        if compact.is_empty() {
            return Err(DiceParseError::Empty);
        }

        let mut terms = Vec::new();
        let mut modifier: i64 = 0;
        for (negative, token) in split_signed(&compact)? {
            if let Some((count, sides)) = token.split_once('d') {
                if negative {
                    return Err(DiceParseError::NegativeDice);
                }
                let count = if count.is_empty() {
                    1
                } else {
                    parse_number(count, token)?
                };
                if count == 0 {
                    return Err(DiceParseError::ZeroCount);
                }
                let sides = parse_number(sides, token)?;
                let die = Die::from_sides(sides).ok_or(DiceParseError::UnsupportedDie { sides })?;
                terms.push(DiceTerm { count, die });
            } else {
                let value = i64::from(parse_number(token, token)?);
                modifier += if negative { -value } else { value };
                if modifier.abs() > i64::from(MAX_MODIFIER) {
                    return Err(DiceParseError::ModifierOutOfRange);
                }
            }
        }

        if terms.is_empty() {
            return Err(DiceParseError::NoDice);
        }
        let expr = DiceExpr {
            terms,
            modifier: modifier as i32,
        };
        if expr.dice_count() > MAX_DICE {
            return Err(DiceParseError::TooManyDice);
        }
        Ok(expr)
    }

    pub fn terms(&self) -> &[DiceTerm] {
        &self.terms
    }

    pub fn modifier(&self) -> i32 {
        self.modifier
    }

    /// Total number of physical dice rolled.
    pub fn dice_count(&self) -> u32 {
        self.terms.iter().map(|t| t.count).sum()
    }

    /// Lowest possible total.
    pub fn min_total(&self) -> i32 {
        self.dice_count() as i32 + self.modifier
    }

    /// Highest possible total.
    pub fn max_total(&self) -> i32 {
        self.terms
            .iter()
            .map(|t| (t.count * t.die.sides()) as i32)
            .sum::<i32>()
            + self.modifier
    }

    /// Returns a copy with `delta` added to the constant modifier.
    pub fn with_extra_modifier(&self, delta: i32) -> Self {
        DiceExpr {
            terms: self.terms.clone(),
            modifier: self.modifier + delta,
        }
    }

    /// The exact chance of each total, `(total, probability)` from lowest to
    /// highest: the physical distribution of the dice (e.g. 2d6 peaks at 7).
    pub fn probabilities(&self) -> Vec<(i32, f64)> {
        // Distribution of the sum of dice, built one die at a time.
        let mut dist = vec![1.0_f64];
        for term in &self.terms {
            let sides = term.die.sides() as usize;
            for _ in 0..term.count {
                let mut next = vec![0.0; dist.len() + sides];
                for (sum, p) in dist.iter().enumerate() {
                    for face in 1..=sides {
                        next[sum + face] += p / sides as f64;
                    }
                }
                dist = next;
            }
        }
        dist.into_iter()
            .enumerate()
            .filter(|(_, p)| *p > 0.0)
            .map(|(sum, p)| (sum as i32 + self.modifier, p))
            .collect()
    }

    /// Rolls every die individually using `faces` and sums the result.
    pub fn roll(&self, faces: &mut dyn FaceSource) -> Result<DiceRoll, FaceError> {
        let mut dice = Vec::with_capacity(self.dice_count() as usize);
        for term in &self.terms {
            for _ in 0..term.count {
                let value = faces.face(term.die)?;
                if !(1..=term.die.sides()).contains(&value) {
                    return Err(FaceError::OutOfRange {
                        die: term.die,
                        value,
                    });
                }
                dice.push(DieRoll {
                    die: term.die,
                    value,
                });
            }
        }
        let total = dice.iter().map(|d| d.value as i32).sum::<i32>() + self.modifier;
        Ok(DiceRoll {
            expression: self.to_string(),
            dice,
            modifier: self.modifier,
            total,
        })
    }
}

fn split_signed(s: &str) -> Result<Vec<(bool, &str)>, DiceParseError> {
    let mut out = Vec::new();
    let mut negative = false;
    let mut start = 0;
    let bytes = s.as_bytes();
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b'+' || bytes[i] == b'-' {
            let token = &s[start..i];
            // Leading, trailing or doubled signs ("+1d6", "1d6+", "2d6++1").
            if token.is_empty() {
                return Err(DiceParseError::InvalidToken {
                    token: s.to_owned(),
                });
            }
            out.push((negative, token));
            if i < bytes.len() {
                negative = bytes[i] == b'-';
            }
            start = i + 1;
        }
    }
    Ok(out)
}

fn parse_number(digits: &str, token: &str) -> Result<u32, DiceParseError> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(DiceParseError::InvalidToken {
            token: token.to_owned(),
        });
    }
    digits.parse().map_err(|_| DiceParseError::InvalidToken {
        token: token.to_owned(),
    })
}

impl FromStr for DiceExpr {
    type Err = DiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DiceExpr::parse(s)
    }
}

/// Canonical form, e.g. `1d12`, `2d6+1`, `1d8+1d4-2`.
impl fmt::Display for DiceExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, term) in self.terms.iter().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            write!(f, "{}{}", term.count, term.die)?;
        }
        match self.modifier {
            0 => Ok(()),
            m if m > 0 => write!(f, "+{m}"),
            m => write!(f, "{m}"),
        }
    }
}

/// One physical die and the face it landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DieRoll {
    pub die: Die,
    pub value: u32,
}

/// The full record of rolling an expression: every die and the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiceRoll {
    /// Canonical expression that was rolled.
    pub expression: String,
    pub dice: Vec<DieRoll>,
    pub modifier: i32,
    pub total: i32,
}

/// `9` for a single die, otherwise `3 + 5 = 8` / `3 + 5 + 1 = 9`.
impl fmt::Display for DiceRoll {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.dice.len() == 1 && self.modifier == 0 {
            return write!(f, "{}", self.total);
        }
        let mut parts: Vec<String> = self.dice.iter().map(|d| d.value.to_string()).collect();
        match self.modifier {
            0 => {}
            m if m > 0 => parts.push(m.to_string()),
            m => parts.push(format!("({m})")),
        }
        write!(f, "{} = {}", parts.join(" + "), self.total)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FaceError {
    #[error("{value} is not a face of a {die}")]
    OutOfRange { die: Die, value: u32 },
    #[error("no more dice values were supplied")]
    Exhausted,
}

/// Where die faces come from: software RNG or values read off physical dice.
pub trait FaceSource {
    /// Returns the face for one roll of `die`, in `1..=die.sides()`.
    fn face(&mut self, die: Die) -> Result<u32, FaceError>;
}

/// Software dice backed by a uniform RNG, one draw per physical die.
pub struct RandomFaces<R: Rng> {
    rng: R,
}

impl<R: Rng> RandomFaces<R> {
    pub fn new(rng: R) -> Self {
        Self { rng }
    }
}

impl RandomFaces<StdRng> {
    /// Deterministic dice for tests and reproducible runs.
    pub fn seeded(seed: u64) -> Self {
        Self::new(StdRng::seed_from_u64(seed))
    }

    /// Dice seeded from the operating system.
    pub fn from_entropy() -> Self {
        Self::new(rand::make_rng())
    }
}

impl<R: Rng> FaceSource for RandomFaces<R> {
    fn face(&mut self, die: Die) -> Result<u32, FaceError> {
        Ok(match die {
            // Percentile dice: a tens d10 (00-90) and a units d10 (0-9).
            Die::D100 => {
                let tens = self.rng.random_range(0..10u32);
                let units = self.rng.random_range(0..10u32);
                match tens * 10 + units {
                    0 => 100,
                    v => v,
                }
            }
            other => self.rng.random_range(1..=other.sides()),
        })
    }
}

/// Faces supplied in order, e.g. entered from physical dice by the user.
#[derive(Debug, Clone, Default)]
pub struct ScriptedFaces {
    values: VecDeque<u32>,
}

impl ScriptedFaces {
    pub fn new(values: impl IntoIterator<Item = u32>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }

    pub fn remaining(&self) -> usize {
        self.values.len()
    }
}

impl FaceSource for ScriptedFaces {
    fn face(&mut self, die: Die) -> Result<u32, FaceError> {
        let value = self.values.pop_front().ok_or(FaceError::Exhausted)?;
        if (1..=die.sides()).contains(&value) {
            Ok(value)
        } else {
            Err(FaceError::OutOfRange { die, value })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> DiceExpr {
        DiceExpr::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn parses_every_supported_single_die() {
        for (s, die) in [
            ("1d4", Die::D4),
            ("1d6", Die::D6),
            ("1d8", Die::D8),
            ("1d10", Die::D10),
            ("1d12", Die::D12),
            ("1d20", Die::D20),
            ("1d100", Die::D100),
        ] {
            let e = parse(s);
            assert_eq!(e.terms(), &[DiceTerm { count: 1, die }]);
            assert_eq!(e.min_total(), 1);
            assert_eq!(e.max_total(), die.sides() as i32);
            assert_eq!(e.to_string(), s);
        }
    }

    #[test]
    fn parses_combinations_and_modifiers() {
        let e = parse("2d6");
        assert_eq!((e.min_total(), e.max_total()), (2, 12));

        let e = parse(" 1D8 + d4 - 2 ");
        assert_eq!(e.to_string(), "1d8+1d4-2");
        assert_eq!((e.min_total(), e.max_total()), (0, 10));

        let e = parse("3d6+1");
        assert_eq!((e.min_total(), e.max_total()), (4, 19));
        assert_eq!(e.dice_count(), 3);
    }

    #[test]
    fn shorthand_without_count_normalises() {
        assert_eq!(parse("d20").to_string(), "1d20");
    }

    #[test]
    fn rejects_invalid_expressions() {
        use DiceParseError::*;
        let cases: &[(&str, DiceParseError)] = &[
            ("", Empty),
            ("   ", Empty),
            ("d7", UnsupportedDie { sides: 7 }),
            ("1d3", UnsupportedDie { sides: 3 }),
            ("0d6", ZeroCount),
            ("5", NoDice),
            ("1d6-1d4", NegativeDice),
            ("21d6", TooManyDice),
            ("10d6+11d4", TooManyDice),
            ("1d6+5000", ModifierOutOfRange),
        ];
        for (input, expected) in cases {
            assert_eq!(DiceExpr::parse(input).as_ref(), Err(expected), "{input:?}");
        }
        for bad in [
            "d", "1d", "xd6", "1d6+", "+1d6", "1d6++1", "2x6", "1d6*2", "1.5d6",
        ] {
            assert!(
                matches!(DiceExpr::parse(bad), Err(InvalidToken { .. })),
                "{bad:?} should be an invalid token"
            );
        }
    }

    #[test]
    fn scripted_roll_records_every_die() {
        let roll = parse("2d6+1")
            .roll(&mut ScriptedFaces::new([3, 5]))
            .unwrap();
        assert_eq!(roll.expression, "2d6+1");
        assert_eq!(
            roll.dice,
            vec![
                DieRoll {
                    die: Die::D6,
                    value: 3
                },
                DieRoll {
                    die: Die::D6,
                    value: 5
                }
            ]
        );
        assert_eq!(roll.total, 9);
        assert_eq!(roll.to_string(), "3 + 5 + 1 = 9");
    }

    #[test]
    fn single_die_displays_plain_value() {
        let roll = parse("1d12").roll(&mut ScriptedFaces::new([9])).unwrap();
        assert_eq!(roll.to_string(), "9");
    }

    #[test]
    fn scripted_faces_reject_impossible_values() {
        let err = parse("1d6").roll(&mut ScriptedFaces::new([7])).unwrap_err();
        assert_eq!(
            err,
            FaceError::OutOfRange {
                die: Die::D6,
                value: 7
            }
        );
        let err = parse("1d6").roll(&mut ScriptedFaces::new([0])).unwrap_err();
        assert!(matches!(err, FaceError::OutOfRange { .. }));
        let err = parse("2d6").roll(&mut ScriptedFaces::new([1])).unwrap_err();
        assert_eq!(err, FaceError::Exhausted);
    }

    #[test]
    fn random_faces_stay_on_the_die_and_hit_every_face() {
        let mut faces = RandomFaces::seeded(7);
        for die in Die::ALL {
            let sides = die.sides();
            let mut seen = vec![false; sides as usize + 1];
            for _ in 0..(sides * 200) {
                let v = faces.face(die).unwrap();
                assert!((1..=sides).contains(&v), "{die} rolled {v}");
                seen[v as usize] = true;
            }
            assert!(seen[1..].iter().all(|&s| s), "{die} never rolled some face");
        }
    }

    #[test]
    fn random_rolls_are_reproducible_with_a_seed() {
        let expr = parse("3d6+1d20");
        let rolls = |seed| {
            let mut faces = RandomFaces::seeded(seed);
            (0..50)
                .map(|_| expr.roll(&mut faces).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(rolls(42), rolls(42));
        assert_ne!(rolls(42), rolls(43));
    }

    #[test]
    fn single_dice_are_uniform() {
        // Chi-square goodness of fit against a uniform distribution.
        let mut faces = RandomFaces::seeded(1234);
        for die in Die::ALL {
            let sides = die.sides() as usize;
            let n = 2000 * sides;
            let mut counts = vec![0u32; sides + 1];
            for _ in 0..n {
                counts[faces.face(die).unwrap() as usize] += 1;
            }
            let expected = (n / sides) as f64;
            let chi2: f64 = counts[1..]
                .iter()
                .map(|&c| (c as f64 - expected).powi(2) / expected)
                .sum();
            // Generous bound: mean is (sides - 1), sd is sqrt(2 * (sides - 1)).
            let df = (sides - 1) as f64;
            assert!(
                chi2 < df + 6.0 * (2.0 * df).sqrt(),
                "{die} chi2 {chi2} too high"
            );
        }
    }

    #[test]
    fn two_d6_has_the_physical_bell_curve() {
        // A hidden uniform 2-12 pick would give each total ~1/11; real 2d6
        // gives 7 six times as often as 2 or 12.
        let expr = parse("2d6");
        let mut faces = RandomFaces::seeded(99);
        let n = 72_000;
        let mut counts = [0u32; 13];
        for _ in 0..n {
            let roll = expr.roll(&mut faces).unwrap();
            assert_eq!(roll.dice.len(), 2);
            assert_eq!(
                roll.total,
                roll.dice.iter().map(|d| d.value as i32).sum::<i32>()
            );
            counts[roll.total as usize] += 1;
        }
        let ways = [0, 0, 1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];
        for total in 2..=12 {
            let expected = n as f64 * ways[total] as f64 / 36.0;
            let deviation = (counts[total] as f64 - expected).abs() / expected;
            assert!(
                deviation < 0.06,
                "total {total}: {} vs {expected}",
                counts[total]
            );
        }
    }

    #[test]
    fn d100_spans_1_to_100() {
        let mut faces = RandomFaces::seeded(5);
        let (mut lo, mut hi) = (u32::MAX, 0);
        for _ in 0..20_000 {
            let v = faces.face(Die::D100).unwrap();
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert_eq!((lo, hi), (1, 100));
    }

    #[test]
    fn probabilities_match_the_physical_dice() {
        let d20 = parse("1d20").probabilities();
        assert_eq!(d20.len(), 20);
        assert!(d20.iter().all(|(_, p)| (p - 0.05).abs() < 1e-12));

        let two_d6 = parse("2d6+1").probabilities();
        assert_eq!(two_d6.first().unwrap().0, 3);
        assert_eq!(two_d6.last().unwrap().0, 13);
        let seven = two_d6.iter().find(|(t, _)| *t == 8).unwrap().1;
        assert!((seven - 6.0 / 36.0).abs() < 1e-12);
        let total: f64 = two_d6.iter().map(|(_, p)| p).sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn with_extra_modifier_shifts_bounds() {
        let e = parse("1d12").with_extra_modifier(2);
        assert_eq!(e.to_string(), "1d12+2");
        assert_eq!((e.min_total(), e.max_total()), (3, 14));
    }
}
