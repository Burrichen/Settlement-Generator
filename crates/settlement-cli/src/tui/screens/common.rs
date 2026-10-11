//! Rendering helpers shared by several screens.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use settlement_core::editor::Coverage;
use settlement_core::engine::Named;
use settlement_core::ids::TagId;
use settlement_core::session::StageOutcome;
use settlement_core::validation::{Severity, ValidationIssue};

use crate::tui::theme::Styles;

/// One stage of a settlement sheet.
pub struct SheetRow {
    pub label: String,
    pub value: String,
    pub source: Option<String>,
    pub tags: Option<Vec<Named<TagId>>>,
}

/// A settlement laid out stage by stage: label, result, the table actually
/// used, and tags (only when given, i.e. with Tag View).
pub fn sheet_lines(rows: &[SheetRow], styles: &Styles) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<18}", row.label.to_uppercase()), styles.dim()),
            Span::styled(row.value.clone(), styles.title()),
            Span::styled(
                row.source
                    .as_ref()
                    .map_or(String::new(), |s| format!("   {s}")),
                styles.dim(),
            ),
        ]));
        if let Some(tags) = row.tags.as_ref().filter(|t| !t.is_empty()) {
            lines.push(tag_line(tags, styles, 18));
        }
    }
    lines
}

/// "Tags: A, B" kept visually secondary.
pub fn tag_line(tags: &[Named<TagId>], styles: &Styles, indent: usize) -> Line<'static> {
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    Line::styled(
        format!("{}tags: {}", " ".repeat(indent), names.join(", ")),
        styles.dim(),
    )
}

/// "Rolled 14 on 1d20", "Chosen", and follow-up rolls.
pub fn outcome_lines(outcome: Option<&StageOutcome>, styles: &Styles) -> Vec<Line<'static>> {
    let Some(o) = outcome else {
        return Vec::new();
    };
    let mut lines = vec![if o.chosen {
        Line::styled("Chosen", styles.dim())
    } else {
        Line::from(vec![
            Span::styled("Rolled ", styles.dim()),
            Span::styled(o.roll.clone().unwrap_or_default(), styles.title()),
            Span::styled(
                format!(
                    " on {}{}",
                    o.dice.clone().unwrap_or_default(),
                    if o.rerolls > 0 {
                        format!(
                            " (after {} reroll{})",
                            o.rerolls,
                            if o.rerolls == 1 { "" } else { "s" }
                        )
                    } else {
                        String::new()
                    }
                ),
                styles.dim(),
            ),
        ])
    }];
    for f in &o.follow_ups {
        lines.push(Line::styled(
            format!(
                "{}: rolled {} on {} → {}",
                f.name,
                f.roll,
                f.dice,
                f.becomes.clone().unwrap_or_else(|| "kept".into())
            ),
            styles.dim(),
        ));
    }
    lines
}

pub fn issue_lines(
    issues: &[ValidationIssue],
    error: Option<&str>,
    styles: &Styles,
) -> Vec<Line<'static>> {
    if error.is_none() && issues.is_empty() {
        return vec![Line::styled("No problems found.", styles.good())];
    }
    let mut lines = Vec::new();
    if let Some(e) = error {
        lines.push(Line::styled(format!("Error  {e}"), styles.error()));
    }
    for issue in issues {
        let (label, style) = match issue.severity {
            Severity::Error => ("Error  ", styles.error()),
            Severity::Warning => ("Warning", styles.dim()),
        };
        lines.push(Line::from(vec![
            Span::styled(label, style.add_modifier(Modifier::BOLD)),
            Span::raw(format!("  {}", issue.message)),
        ]));
    }
    lines
}

/// Every face of the dice; uncovered faces in red, overlaps in the accent.
pub fn coverage_lines(
    coverage: &Coverage,
    name_of: &dyn Fn(&str) -> String,
    width: u16,
    styles: &Styles,
) -> Vec<Line<'static>> {
    let ok = coverage.dice_error.is_none()
        && coverage.uncovered.is_empty()
        && coverage.overlapping.is_empty()
        && coverage.impossible.is_empty();
    let mut summary = vec![
        Span::styled(coverage.label.to_uppercase(), styles.dim()),
        Span::raw(format!("  {}  ", coverage.dice)),
    ];
    if let Some(e) = &coverage.dice_error {
        summary.push(Span::styled(e.clone(), styles.error()));
    } else if ok {
        summary.push(Span::styled("every face covered once", styles.good()));
    } else {
        let mut problems = Vec::new();
        if !coverage.uncovered.is_empty() {
            let r: Vec<String> = coverage.uncovered.iter().map(|r| r.to_string()).collect();
            problems.push(format!("uncovered {}", r.join(", ")));
        }
        if !coverage.overlapping.is_empty() {
            let r: Vec<String> = coverage.overlapping.iter().map(|r| r.to_string()).collect();
            problems.push(format!("overlapping {}", r.join(", ")));
        }
        for i in &coverage.impossible {
            problems.push(format!(
                "{} ({}) cannot be rolled",
                name_of(&i.result),
                i.range
            ));
        }
        summary.push(Span::styled(problems.join("; "), styles.error()));
    }
    let mut lines = vec![Line::from(summary)];

    // Faces, wrapped to the width.
    let mut row: Vec<Span> = Vec::new();
    let mut used = 0u16;
    for face in &coverage.faces {
        let text = format!("{:>3}", face.face);
        let style = match face.claimed_by.len() {
            0 => styles
                .error()
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            1 => styles.dim(),
            _ => ratatui::style::Style::new()
                .fg(styles.accent)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        };
        if used + 4 > width && !row.is_empty() {
            lines.push(Line::from(std::mem::take(&mut row)));
            used = 0;
        }
        row.push(Span::styled(text, style));
        row.push(Span::raw(" "));
        used += 4;
    }
    if !row.is_empty() {
        lines.push(Line::from(row));
    }
    lines
}

/// Percentage for a chance, e.g. "15%".
pub fn percent(p: f64) -> String {
    if p == 0.0 {
        "0%".into()
    } else if p < 0.001 {
        "<0.1%".into()
    } else if p < 0.1 {
        format!("{:.1}%", p * 100.0)
    } else {
        format!("{:.0}%", p * 100.0)
    }
}

/// Parses "5", "1-10" or "1–10"; empty means no faces.
pub fn parse_range(text: &str) -> Result<Option<settlement_core::content::FaceRange>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = t.split(['-', '–']).map(str::trim).collect();
    let num = |s: &str| {
        s.parse::<i32>()
            .map_err(|_| format!("`{s}` is not a number"))
    };
    match parts.as_slice() {
        [a] => Ok(Some(settlement_core::content::FaceRange::single(num(a)?))),
        [a, b] => Ok(Some(settlement_core::content::FaceRange::new(
            num(a)?,
            num(b)?,
        ))),
        _ => Err("Use a number, or two numbers like 1-10.".into()),
    }
}
