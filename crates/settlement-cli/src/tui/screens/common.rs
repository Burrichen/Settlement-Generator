//! Rendering helpers shared by several screens.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use settlement_core::editor::Coverage;
use settlement_core::engine::Named;
use settlement_core::figures::{AuthorityFigure, AuthorityFigures, FigureName};
use settlement_core::framework::StepRecord;
use settlement_core::ids::TagId;
use settlement_core::locations::{NotableLocation, NotableLocations};
use settlement_core::session::{StageOutcome, StageStatus, StageView};
use settlement_core::validation::{Severity, ValidationIssue};

use crate::tui::theme::Styles;

/// One stage of a settlement sheet.
pub struct SheetRow {
    pub label: String,
    pub value: String,
    pub source: Option<String>,
    pub tags: Option<Vec<Named<TagId>>>,
    /// Elaborates the row before it: shown as a further line of its value.
    pub sub_stage: bool,
    /// Did not apply. A skipped sub-stage is left out.
    pub skipped: bool,
    /// The Authority Figures, on that stage's row.
    pub figures: Option<AuthorityFigures>,
    /// The Notable Locations, on that stage's row.
    pub locations: Option<NotableLocations>,
    /// What a flow stage produced (objects, repeats and so on).
    pub flow: Option<StepRecord>,
}

/// A flow stage's record, one indented line per roll, object or repeat.
pub fn flow_lines(record: &StepRecord, styles: &Styles, indent: usize) -> Vec<Line<'static>> {
    record
        .outline_below()
        .into_iter()
        .map(|l| {
            let pad = " ".repeat(indent + 2 * l.depth);
            if l.value.is_empty() {
                Line::from(vec![Span::raw(pad), Span::styled(l.label, styles.dim())])
            } else {
                Line::from(vec![
                    Span::raw(pad),
                    Span::styled(format!("{}: ", l.label), styles.dim()),
                    Span::styled(l.value, styles.title()),
                ])
            }
        })
        .collect()
}

/// One Notable Location: "Tavern · Hospitality · 4 spare rooms · District 1".
pub fn location_label(record: &NotableLocations, location: &NotableLocation) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(subtype) = &location.subtype {
        parts.push(subtype.name.clone());
    }
    parts.push(location.location_type.name.clone());
    if let Some(rooms) = location.spare_rooms {
        parts.push(format!("{rooms} spare rooms"));
    }
    if let Some(district) = record
        .areas
        .iter()
        .filter_map(|a| a.district.as_ref())
        .find(|d| Some(&d.id) == location.district.as_ref())
    {
        parts.push(district.label.clone());
    }
    parts.join(" · ")
}

/// The Notable Locations as a numbered list (the guaranteed Tavern first),
/// after the District Count if any.
pub fn location_lines(
    record: &NotableLocations,
    styles: &Styles,
    indent: usize,
) -> Vec<Line<'static>> {
    let pad = " ".repeat(indent);
    let mut lines = Vec::new();
    if let Some(districts) = &record.districts {
        lines.push(Line::styled(
            format!("{pad}{}", districts.roll.result.name),
            styles.title(),
        ));
    }
    for (i, location) in record.locations.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!("{pad}{:>2}. ", i + 1), styles.dim()),
            Span::styled(location_label(record, location), styles.title()),
        ]));
    }
    lines
}

/// One field's result, e.g. a figure's species.
fn field(figure: &AuthorityFigure, id: &str) -> String {
    figure
        .field(id)
        .map_or_else(|| "—".to_owned(), |r| r.result.name.clone())
}

/// The Authority Figures and their relationships, compactly: the name in
/// bold, species • gender • age, then the other details indented.
pub fn figure_lines(
    record: &AuthorityFigures,
    styles: &Styles,
    indent: usize,
    show_tags: bool,
) -> Vec<Line<'static>> {
    let pad = " ".repeat(indent);
    let mut lines = Vec::new();
    for figure in &record.figures {
        // The Role / Title, then the name.
        if let Some(role) = &figure.role {
            lines.push(Line::styled(
                format!("{pad}{}", role.label),
                styles.heading(),
            ));
        }
        lines.push(Line::from(vec![
            Span::raw(pad.clone()),
            Span::styled(
                figure.label.clone(),
                styles.title().add_modifier(Modifier::BOLD),
            ),
        ]));
        if let FigureName::Missing { reason } = &figure.name {
            lines.push(Line::styled(format!("{pad}  {reason}"), styles.dim()));
        }
        lines.push(Line::styled(
            format!(
                "{pad}  {} • {} • {}",
                field(figure, "species"),
                field(figure, "gender"),
                field(figure, "age")
            ),
            styles.dim(),
        ));
        for (id, label) in [
            ("disposition", "Disposition"),
            ("motive", "Motive"),
            ("reputation", "Reputation"),
            ("flaw", "Flaw"),
        ] {
            lines.push(Line::from(vec![
                Span::styled(format!("{pad}  {label}: "), styles.dim()),
                Span::raw(field(figure, id)),
            ]));
        }
        // Name provenance, like tags, only with Tag View.
        if show_tags && let FigureName::Generated { scheme, parts, .. } = &figure.name {
            let parts: String = parts
                .iter()
                .map(|p| format!(" · {}: {}", p.label, p.result.name))
                .collect();
            lines.push(Line::styled(
                format!("{pad}  named from {}{parts}", scheme.name),
                styles.dim(),
            ));
        }
        if show_tags && !figure.tags.is_empty() {
            lines.push(tag_line(&figure.tags, styles, indent + 2));
        }
    }
    if !record.relationships.is_empty() {
        lines.push(Line::styled(format!("{pad}RELATIONSHIPS"), styles.dim()));
        let label = |id| {
            record
                .figure(id)
                .map_or_else(|| id.to_string(), |f| f.label.clone())
        };
        for r in &record.relationships {
            lines.push(Line::from(vec![
                Span::raw(format!("{pad}  {} ↔ {} — ", label(&r.a), label(&r.b))),
                Span::styled(r.roll.result.name.clone(), styles.title()),
            ]));
        }
    }
    lines
}

/// A sub-stage that was skipped does not apply, and is not shown.
pub fn applies(stage: &StageView) -> bool {
    !(stage.sub_stage && stage.status == StageStatus::Skipped)
}

/// A settlement laid out stage by stage: label, result, the table actually
/// used, and tags (only when given, i.e. with Tag View). Each sub-stage that
/// applies becomes a further line of the row before it (e.g. "Local
/// Council" then "4 Representatives").
pub fn sheet_lines(rows: &[SheetRow], styles: &Styles) -> Vec<Line<'static>> {
    /// A row with its sub-stages folded in.
    struct Folded<'a> {
        row: &'a SheetRow,
        details: Vec<String>,
        tags: Option<Vec<Named<TagId>>>,
    }
    let mut folded: Vec<Folded> = Vec::new();
    for row in rows {
        match folded.last_mut() {
            Some(parent) if row.sub_stage => {
                if !row.skipped {
                    parent.details.push(row.value.clone());
                    if let Some(more) = &row.tags {
                        parent
                            .tags
                            .get_or_insert_with(Vec::new)
                            .extend(more.iter().cloned());
                    }
                }
            }
            _ => folded.push(Folded {
                row,
                details: Vec::new(),
                tags: row.tags.clone(),
            }),
        }
    }
    let mut lines = Vec::new();
    for Folded { row, details, tags } in folded {
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
        for detail in details {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(18)),
                Span::styled(detail, styles.title()),
            ]));
        }
        if let Some(tags) = tags.as_ref().filter(|t| !t.is_empty()) {
            lines.push(tag_line(tags, styles, 18));
        }
        if let Some(figures) = &row.figures {
            lines.extend(figure_lines(figures, styles, 18, row.tags.is_some()));
        }
        if let Some(locations) = &row.locations {
            lines.extend(location_lines(locations, styles, 18));
        }
        if let Some(flow) = &row.flow {
            lines.extend(flow_lines(flow, styles, 18));
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

/// How each value of a stage was reached (the Roll details setting).
pub fn trace_lines(
    traces: Option<&Vec<settlement_core::trace::RollTrace>>,
    styles: &Styles,
) -> Vec<Line<'static>> {
    let Some(traces) = traces.filter(|t| !t.is_empty()) else {
        return Vec::new();
    };
    let mut lines = vec![Line::default(), Line::styled("ROLL DETAILS", styles.dim())];
    for t in traces {
        lines.push(Line::styled(t.title.clone(), styles.title()));
        lines.extend(
            t.lines
                .iter()
                .map(|l| Line::styled(format!("  {l}"), styles.dim())),
        );
    }
    lines
}
