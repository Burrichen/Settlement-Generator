//! Drawing one stored map as an SVG document, for export. Both interfaces
//! export through this, so a map looks the same whichever made the file.
//!
//! - One map per document: a City's or Capital's districts are drawn one at
//!   a time, never joined into an invented city image.
//! - Drawn from stored geometry only; nothing is regenerated.
//! - Markers (pins, numbered as in the legend) and a legend panel are
//!   optional.
//! - Colours are a fixed light palette that prints well; they do not follow
//!   the application theme.

use std::fmt::Write as _;

use super::geometry::Point;
use super::model::{
    GroundKind, MapLegendEntry, MapMarker, MarkerTarget, SettlementMap, StreetKind,
};

/// What to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RenderOptions {
    /// Numbered pins for Notable Locations and points of interest.
    pub markers: bool,
    /// A legend panel beside the map: title, key and numbered entries.
    pub legend: bool,
    /// Width of the map area in pixels (the legend panel is extra).
    pub width_px: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            markers: true,
            legend: true,
            width_px: 1600,
        }
    }
}

/// Smallest and largest map widths accepted.
pub const MIN_WIDTH_PX: u32 = 300;
pub const MAX_WIDTH_PX: u32 = 8000;
const LEGEND_PX: f64 = 360.0;
const PAD: f64 = 24.0;
const FONT: &str = "Segoe UI, Noto Sans, DejaVu Sans, Arial, sans-serif";

struct Palette {
    paper: &'static str,
    ink: &'static str,
    muted: &'static str,
    field: &'static str,
    water: &'static str,
    tree: &'static str,
    road: &'static str,
    street: &'static str,
    lane: &'static str,
    square: &'static str,
    building: &'static str,
    building_edge: &'static str,
    wall: &'static str,
    marker: &'static str,
}

const PALETTE: Palette = Palette {
    paper: "#f4ecd8",
    ink: "#2b2118",
    muted: "#6e5f4f",
    field: "#e2dcb2",
    water: "#a9c7d6",
    tree: "#a8bb8a",
    road: "#cdb98f",
    street: "#d8c8a3",
    lane: "#e0d4b4",
    square: "#e6dcc3",
    building: "#8f7a62",
    building_edge: "#3d3126",
    wall: "#5d5146",
    marker: "#a3402c",
};

fn ground(kind: GroundKind) -> &'static str {
    use GroundKind::*;
    match kind {
        Sand | Rock | Wasteland | Hills => "#efe2c4",
        Snow | Ice => "#eef0ee",
        Marsh | Jungle => "#e3e8cf",
        Crystal | Ichor | Cavern => "#e9e1e8",
        _ => "#ecebd0",
    }
}

/// Escapes text for XML content and attributes (names may be any Unicode).
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Maps metres (the map frame) to pixels of the drawing.
struct Frame {
    scale: f64,
    min_x: f64,
    min_y: f64,
}

impl Frame {
    fn x(&self, x: f64) -> f64 {
        PAD + (x - self.min_x) * self.scale
    }
    fn y(&self, y: f64) -> f64 {
        PAD + (y - self.min_y) * self.scale
    }
    fn pt(&self, p: Point) -> (f64, f64) {
        (self.x(p.0), self.y(p.1))
    }
}

fn path(frame: &Frame, points: &[Point], closed: bool) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let (x, y) = frame.pt(*p);
        let _ = write!(d, "{}{x:.1} {y:.1}", if i == 0 { "M" } else { "L" });
    }
    if closed && !d.is_empty() {
        d.push('Z');
    }
    d
}

fn circles(frame: &Frame, centres: impl Iterator<Item = (Point, f64)>) -> String {
    let mut d = String::new();
    for (c, r) in centres {
        let (x, y) = frame.pt(c);
        let r = r * frame.scale;
        let _ = write!(
            d,
            "M{:.1} {y:.1}a{r:.1} {r:.1} 0 1 0 {:.1} 0a{r:.1} {r:.1} 0 1 0 {:.1} 0",
            x - r,
            2.0 * r,
            -2.0 * r
        );
    }
    d
}

/// The SVG document of one map. `title` heads the legend (for example the
/// settlement and district names).
pub fn svg(
    map: &SettlementMap,
    title: &str,
    markers: &[MapMarker],
    legend: &[MapLegendEntry],
    options: RenderOptions,
) -> String {
    let p = &PALETTE;
    let g = &map.geometry;
    let b = g.bounds;
    let width_px = options.width_px.clamp(MIN_WIDTH_PX, MAX_WIDTH_PX) as f64;
    let frame = Frame {
        scale: width_px / b.width(),
        min_x: b.min_x,
        min_y: b.min_y,
    };
    let map_h = b.height() * frame.scale;
    let legend_w = if options.legend { LEGEND_PX } else { 0.0 };
    let total_w = width_px + 2.0 * PAD + legend_w;
    let mut total_h = map_h + 2.0 * PAD;

    let mut body = String::new();
    let w = |px: f64| px * frame.scale;
    // Ground and land use.
    let _ = write!(
        body,
        r#"<rect x="{PAD}" y="{PAD}" width="{width_px:.1}" height="{map_h:.1}" fill="{}"/>"#,
        ground(g.terrain.ground)
    );
    let fields: String = g
        .terrain
        .fields
        .iter()
        .map(|f| path(&frame, &f.area, true))
        .collect();
    let _ = write!(body, r#"<path d="{fields}" fill="{}"/>"#, p.field);
    let water: String = g
        .water
        .iter()
        .map(|x| path(&frame, &x.area, true))
        .collect();
    let _ = write!(body, r#"<path d="{water}" fill="{}"/>"#, p.water);
    let trees = circles(
        &frame,
        g.terrain.trees.iter().map(|t| (t.centre, t.radius_m)),
    );
    let _ = write!(
        body,
        r#"<path d="{trees}" fill="{}" opacity="0.8"/>"#,
        p.tree
    );
    let squares: String = g
        .terrain
        .squares
        .iter()
        .map(|s| path(&frame, &s.area, true))
        .collect();
    let _ = write!(body, r#"<path d="{squares}" fill="{}"/>"#, p.square);
    // Streets, widest kind first.
    for (kind, colour) in [
        (StreetKind::Road, p.road),
        (StreetKind::Street, p.street),
        (StreetKind::Lane, p.lane),
    ] {
        let streets: Vec<_> = g.streets.iter().filter(|s| s.kind == kind).collect();
        if streets.is_empty() {
            continue;
        }
        let width = streets.iter().map(|s| s.width_m).fold(0.0, f64::max);
        let d: String = streets
            .iter()
            .map(|s| path(&frame, &s.path, false))
            .collect();
        let _ = write!(
            body,
            r#"<path d="{d}" fill="none" stroke="{colour}" stroke-width="{:.1}" stroke-linecap="round" stroke-linejoin="round"/>"#,
            w(width).max(1.0)
        );
    }
    let bridges = circles(
        &frame,
        g.water
            .iter()
            .flat_map(|x| x.bridges.iter().map(|c| (*c, 3.0))),
    );
    let _ = write!(body, r#"<path d="{bridges}" fill="{}"/>"#, p.wall);
    // Buildings: one path, as on screen.
    let buildings: String = g
        .buildings
        .iter()
        .map(|x| path(&frame, &x.footprint, true))
        .collect();
    let _ = write!(
        body,
        r#"<path d="{buildings}" fill="{}" stroke="{}" stroke-width="{:.2}"/>"#,
        p.building,
        p.building_edge,
        w(0.3).clamp(0.4, 2.0)
    );
    let walls: String = g
        .walls
        .iter()
        .flat_map(|x| x.segments.iter().map(|s| path(&frame, s, false)))
        .collect();
    let _ = write!(
        body,
        r#"<path d="{walls}" fill="none" stroke="{}" stroke-width="{:.1}" stroke-linejoin="round"/>"#,
        p.wall,
        w(2.0).max(1.5)
    );
    let towers = circles(
        &frame,
        g.walls
            .iter()
            .flat_map(|x| x.towers.iter().map(|c| (*c, 2.5))),
    );
    let gates = circles(
        &frame,
        g.walls
            .iter()
            .flat_map(|x| x.gates.iter().map(|c| (*c, 3.0))),
    );
    let _ = write!(body, r#"<path d="{towers}{gates}" fill="{}"/>"#, p.wall);
    // Scale bar: a round length near a fifth of the width.
    let target = b.width() / 5.0;
    let step = 10f64.powf(target.log10().floor());
    let metres = [5.0, 2.0, 1.0]
        .iter()
        .map(|m| m * step)
        .find(|v| *v <= target)
        .unwrap_or(step);
    let bar = metres * frame.scale;
    let (sx, sy) = (PAD + 16.0, PAD + map_h - 18.0);
    let _ = write!(
        body,
        r#"<g font-family="{FONT}" font-size="14" fill="{ink}"><path d="M{sx:.1} {sy:.1}h{bar:.1}" stroke="{ink}" stroke-width="3"/><text x="{tx:.1}" y="{ty:.1}">{label}</text></g>"#,
        ink = p.ink,
        tx = sx,
        ty = sy - 8.0,
        label = if metres >= 1000.0 {
            format!("{} km", metres / 1000.0)
        } else {
            format!("{metres} m")
        }
    );
    // Markers: pins in pixels, the tip on the point, numbered.
    if options.markers {
        for m in markers {
            let (x, y) = frame.pt(m.position);
            let head = match m.target {
                MarkerTarget::NotableLocation { .. } => format!(
                    r#"<circle cx="0" cy="-22" r="12" fill="{}" stroke="{}" stroke-width="2"/>"#,
                    p.marker, p.paper
                ),
                MarkerTarget::PointOfInterest { .. } => format!(
                    r#"<rect x="-10" y="-32" width="20" height="20" transform="rotate(45 0 -22)" fill="{}" stroke="{}" stroke-width="2.5"/>"#,
                    p.paper, p.marker
                ),
            };
            let text = match m.target {
                MarkerTarget::NotableLocation { .. } => p.paper,
                MarkerTarget::PointOfInterest { .. } => p.ink,
            };
            let _ = write!(
                body,
                r#"<g transform="translate({x:.1} {y:.1})"><path d="M0 0V-10" stroke="{ink}" stroke-width="2"/><circle r="2.5" fill="{ink}"/>{head}<text y="-17.5" text-anchor="middle" font-family="{FONT}" font-size="13" font-weight="700" fill="{text}">{n}</text></g>"#,
                ink = p.ink,
                n = m.number
            );
        }
    }
    // Legend panel.
    if options.legend {
        let x0 = PAD * 2.0 + width_px;
        let mut y = PAD + 28.0;
        let mut lines = String::new();
        let _ = write!(
            lines,
            r#"<text x="{x0}" y="{y}" font-size="22" font-weight="700" fill="{}">{}</text>"#,
            p.ink,
            escape(title)
        );
        y += 26.0;
        let _ = write!(
            lines,
            r#"<text x="{x0}" y="{y}" font-size="13" fill="{}">{} buildings · {} {}</text>"#,
            p.muted,
            g.buildings.len(),
            escape(&map.engine.id),
            escape(&map.engine.version)
        );
        y += 32.0;
        let mut key = vec![("Building", p.building)];
        if g.streets.iter().any(|s| s.kind == StreetKind::Road) {
            key.push(("Road", p.road));
        }
        if !g.water.is_empty() {
            key.push(("Water", p.water));
        }
        if !g.walls.is_empty() {
            key.push(("Wall", p.wall));
        }
        if !g.terrain.fields.is_empty() {
            key.push(("Field", p.field));
        }
        if !g.terrain.trees.is_empty() {
            key.push(("Trees", p.tree));
        }
        for (label, colour) in key {
            let _ = write!(
                lines,
                r#"<rect x="{x0}" y="{:.1}" width="18" height="12" fill="{colour}" stroke="{}"/><text x="{:.1}" y="{y}" font-size="14" fill="{}">{label}</text>"#,
                y - 11.0,
                p.building_edge,
                x0 + 28.0,
                p.ink
            );
            y += 22.0;
        }
        if options.markers && !legend.is_empty() {
            y += 14.0;
            let _ = write!(
                lines,
                r#"<text x="{x0}" y="{y}" font-size="15" font-weight="700" fill="{}">Numbered locations</text>"#,
                p.ink
            );
            y += 26.0;
            for e in legend {
                let poi = matches!(e.target, MarkerTarget::PointOfInterest { .. });
                // The same shapes as the pins: round for locations, a
                // diamond for points of interest.
                let (cx, cy) = (x0 + 11.0, y - 5.0);
                let (badge, number_fill) = if poi {
                    (
                        format!(
                            r#"<rect x="{:.1}" y="{:.1}" width="15" height="15" transform="rotate(45 {cx:.1} {cy:.1})" fill="{}" stroke="{}" stroke-width="2"/>"#,
                            cx - 7.5,
                            cy - 7.5,
                            p.paper,
                            p.marker
                        ),
                        p.ink,
                    )
                } else {
                    (
                        format!(
                            r#"<circle cx="{cx:.1}" cy="{cy:.1}" r="11" fill="{}"/>"#,
                            p.marker
                        ),
                        p.paper,
                    )
                };
                // The type, unless the name already says it.
                let detail = if poi {
                    Some("Point of interest")
                } else {
                    e.location_type.as_deref().filter(|t| *t != e.label)
                };
                let _ = write!(
                    lines,
                    r#"{badge}<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="700" fill="{number_fill}">{}</text><text x="{:.1}" y="{y}" font-size="14" fill="{}">{}</text>"#,
                    y - 1.0,
                    e.number,
                    x0 + 30.0,
                    p.ink,
                    escape(&e.label),
                );
                if let Some(detail) = detail {
                    let _ = write!(
                        lines,
                        r#"<text x="{:.1}" y="{:.1}" font-size="12" fill="{}">{}</text>"#,
                        x0 + 30.0,
                        y + 16.0,
                        p.muted,
                        escape(detail)
                    );
                    y += 40.0;
                } else {
                    y += 28.0;
                }
            }
        }
        total_h = total_h.max(y + PAD);
        let _ = write!(body, r#"<g font-family="{FONT}">{lines}</g>"#);
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{total_w:.0}" height="{total_h:.0}" viewBox="0 0 {total_w:.0} {total_h:.0}"><title>{title}</title><rect width="100%" height="100%" fill="{paper}"/>{body}</svg>
"#,
        title = escape(title),
        paper = p.paper
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_for_xml() {
        assert_eq!(
            escape("A & B <c> \"d\" 'e'"),
            "A &amp; B &lt;c&gt; &quot;d&quot; &apos;e&apos;"
        );
        assert_eq!(escape("Ærøskøbing – 城\u{1}"), "Ærøskøbing – 城");
    }
}
