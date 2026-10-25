//! Maps of a saved settlement, in the terminal: the same operations as the
//! GUI's Map tab, through the same service, without drawing the map.
//!
//! - **Maps**: the settlement's map, or one per district (City, Capital),
//!   with status, counts and the numbered legend. Generate or redraw one,
//!   rename or delete a district's map, and export pictures or data.
//! - **Buildings…** picks a building by ID or details and opens it: its
//!   details are generated the first time, then edited field by field.
//! - **Locations…** places, moves, unplaces and renames Notable Locations.
//! - **Points…** adds points of interest by coordinates (metres from the
//!   map's centre, x east, y south), and moves, renames or deletes them.
//!
//! Polygons are never drawn in the terminal; coordinates and IDs are shown
//! instead.

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use settlement_core::ids::{BuildingId, MapId};
use settlement_core::maps::details::{BuildingField, FieldOrigin};
use settlement_core::maps::export::ExportFormat;
use settlement_core::maps::geometry::Point;
use settlement_core::maps::render::RenderOptions;
use settlement_core::maps::{
    AssignOptions, BuildingDetailsView, BuildingEdits, LocationEdit, LocationEntry,
    MapGenerationOptions, MapScope, MarkerTarget, PlaceOn, SettlementMap, UserValue,
};
use settlement_core::service::{MapExportRequest, PlannedMapView, SettlementMapsView};
use settlement_core::settlements::SettlementId;

use crate::tui::widgets::{Modal, ModalResult, PickItem, Row, RowsState};
use crate::tui::{Ctx, Nav, Screen};

// ----- helpers ------------------------------------------------------------------

/// "Building 12", from the ID's number.
fn building_label(id: &BuildingId) -> String {
    id.as_str()
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u32>().ok())
        .map_or_else(|| id.to_string(), |n| format!("Building {n}"))
}

/// "12.5 -3" (metres), as typed and shown.
fn point_text(p: Point) -> String {
    format!("{} {}", p.0, p.1)
}

/// Reads "x y" (metres from the centre; commas allowed).
fn parse_point(text: &str) -> Result<Point, String> {
    let parts: Vec<f64> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().map_err(|_| ()))
        .collect::<Result<_, _>>()
        .map_err(|_| "Enter two numbers: x and y in metres, e.g. 12 -30.".to_string())?;
    match parts.as_slice() {
        [x, y] if x.is_finite() && y.is_finite() => Ok(Point(*x, *y)),
        _ => Err("Enter two numbers: x and y in metres, e.g. 12 -30.".into()),
    }
}

/// Explicit consent for a placement the core refused without it.
fn consent(error: &str) -> Option<AssignOptions> {
    if error.contains("already on") {
        Some(AssignOptions {
            move_location: true,
            replace_occupant: false,
        })
    } else if error.contains("already holds") {
        Some(AssignOptions {
            move_location: false,
            replace_occupant: true,
        })
    } else {
        None
    }
}

fn location_status(l: &LocationEntry) -> String {
    match &l.placement {
        Some(p) if p.missing => "its building was redrawn".into(),
        Some(p) => format!(
            "on {}, {}",
            building_label(&p.building_id),
            p.district.as_ref().map_or(&p.map_label, |d| &d.label)
        ),
        None => "not placed".into(),
    }
}

// ----- the maps screen ------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Generate,
    Redraw,
    Buildings,
    Locations,
    Points,
    Export,
    Rename,
    DeleteMap,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Generate => "Generate",
            Action::Redraw => "Redraw",
            Action::Buildings => "Buildings…",
            Action::Locations => "Locations…",
            Action::Points => "Points…",
            Action::Export => "Export…",
            Action::Rename => "Rename…",
            Action::DeleteMap => "Delete map",
        }
    }
}

/// What an open modal is for.
enum Pending {
    Redraw,
    Building(Vec<BuildingId>),
    Location(Vec<String>),
    LocationAction(String, Vec<&'static str>),
    Place(String, Vec<BuildingId>),
    Consent(String, PlaceOn, AssignOptions),
    RenameLocation(String, u32),
    Point(Vec<Option<String>>),
    PointAction(String, Vec<&'static str>),
    PointAt(Option<String>),
    PointName(Point),
    RenamePoint(String),
    DeletePoint(String),
    Export(Vec<(bool, Vec<ExportFormat>)>),
    RenameDistrict(String),
    DeleteMap,
}

pub struct MapsScreen {
    id: SettlementId,
    name: String,
    view: Option<SettlementMapsView>,
    state: RowsState,
    in_actions: bool,
    action: usize,
    scroll: u16,
    error: Option<String>,
    modal: Option<(Modal, Pending)>,
}

impl MapsScreen {
    pub fn new(ctx: &mut Ctx, id: &SettlementId, name: &str) -> Self {
        let mut screen = MapsScreen {
            id: id.clone(),
            name: name.to_owned(),
            view: None,
            state: RowsState::default(),
            in_actions: false,
            action: 0,
            scroll: 0,
            error: None,
            modal: None,
        };
        screen.reload(ctx);
        screen
    }

    fn reload(&mut self, ctx: &mut Ctx) {
        match ctx.service.maps_get(&self.id) {
            Ok(v) => self.view = Some(v),
            Err(e) => self.error = Some(e),
        }
    }

    /// Applies a changed maps view, or shows why the change failed.
    fn apply(&mut self, ctx: &mut Ctx, result: Result<SettlementMapsView, String>, done: &str) {
        match result {
            Ok(v) => {
                self.view = Some(v);
                self.error = None;
                ctx.flash = Some(done.into());
            }
            Err(e) => {
                self.error = Some(e);
                self.reload(ctx);
            }
        }
    }

    fn planned(&self) -> &[PlannedMapView] {
        self.view.as_ref().map_or(&[], |v| v.planned.as_slice())
    }

    fn rows(&self) -> Vec<Row<usize>> {
        self.planned()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let status = if !p.generated {
                    "no map".to_string()
                } else if p.stale {
                    "changed".into()
                } else {
                    format!(
                        "{} buildings",
                        self.map(&p.id).map_or(0, |m| m.geometry.buildings.len())
                    )
                };
                Row::item(format!("{}  ·  {status}", p.label), i)
            })
            .collect()
    }

    fn current(&self) -> Option<&PlannedMapView> {
        let rows = self.rows();
        self.state
            .current(&rows)
            .and_then(|&i| self.planned().get(i))
    }

    fn map(&self, id: &MapId) -> Option<&SettlementMap> {
        self.view.as_ref()?.maps.as_ref()?.map(id)
    }

    fn actions(&self) -> Vec<Action> {
        let Some(p) = self.current() else {
            return vec![];
        };
        let district = matches!(p.scope, MapScope::District { .. });
        let mut out = vec![];
        if p.generated {
            out.extend([
                Action::Buildings,
                Action::Locations,
                Action::Points,
                Action::Export,
                Action::Redraw,
            ]);
        } else if self.view.as_ref().is_some_and(|v| v.problem.is_none()) {
            out.push(Action::Generate);
        }
        if district {
            out.push(Action::Rename);
            if p.generated {
                out.push(Action::DeleteMap);
            }
        }
        out
    }

    fn generate(&mut self, ctx: &mut Ctx) {
        let Some(p) = self.current().cloned() else {
            return;
        };
        let single = matches!(p.scope, MapScope::Settlement);
        let options = MapGenerationOptions {
            only: (!single).then(|| p.id.clone()),
            ..Default::default()
        };
        let result = ctx.service.maps_generate(&self.id, &options);
        self.apply(ctx, result, &format!("Drew the map of {}", p.label));
    }

    fn run(&mut self, action: Action, ctx: &mut Ctx) -> Nav {
        let Some(p) = self.current().cloned() else {
            return Nav::Stay;
        };
        let Some(view) = self.view.clone() else {
            return Nav::Stay;
        };
        match action {
            Action::Generate => self.generate(ctx),
            Action::Redraw => {
                self.modal = Some((
                    Modal::confirm(
                        &format!("Redraw the map of {}? Its buildings get new IDs.", p.label),
                        "Redraw",
                    ),
                    Pending::Redraw,
                ));
            }
            Action::Buildings => {
                let Some(map) = self.map(&p.id) else {
                    return Nav::Stay;
                };
                let summaries = &view.buildings;
                let items = map
                    .geometry
                    .buildings
                    .iter()
                    .map(|b| {
                        let known = summaries.iter().find(|s| s.building_id == b.id);
                        let detail = known
                            .map(|s| {
                                [s.name.clone(), s.building_type.clone()]
                                    .into_iter()
                                    .flatten()
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                            })
                            .unwrap_or_else(|| "not generated".into());
                        PickItem::new(building_label(&b.id)).hint(format!("{detail}  ({})", b.id))
                    })
                    .collect();
                let ids = map
                    .geometry
                    .buildings
                    .iter()
                    .map(|b| b.id.clone())
                    .collect();
                self.modal = Some((
                    Modal::pick("Open a building (type to search)", items, 0),
                    Pending::Building(ids),
                ));
            }
            Action::Locations => {
                if view.locations.is_empty() {
                    ctx.flash = Some("This settlement has no Notable Locations.".into());
                    return Nav::Stay;
                }
                let items = view
                    .locations
                    .iter()
                    .map(|l| {
                        let number = l.marker.map_or("  ".into(), |n| format!("{n:>2}"));
                        PickItem::new(format!("{number}  {}", l.name)).hint(format!(
                            "{}{} · {}",
                            l.location_type,
                            l.district
                                .as_ref()
                                .map_or(String::new(), |d| format!(" · {}", d.label)),
                            location_status(l)
                        ))
                    })
                    .collect();
                let ids = view.locations.iter().map(|l| l.id.clone()).collect();
                self.modal = Some((
                    Modal::pick("Notable Locations", items, 0),
                    Pending::Location(ids),
                ));
            }
            Action::Points => {
                let mine: Vec<_> = view.pois.iter().filter(|x| x.map_id == p.id).collect();
                let mut items = vec![PickItem::new("Add a point of interest…")];
                let mut ids = vec![None];
                for poi in &mine {
                    items.push(
                        PickItem::new(format!("{:>2}  {}", poi.number, poi.label))
                            .hint(format!("at {}", point_text(poi.position))),
                    );
                    ids.push(Some(poi.id.to_string()));
                }
                self.modal = Some((
                    Modal::pick(&format!("Points of interest on {}", p.label), items, 0),
                    Pending::Point(ids),
                ));
            }
            Action::Export => {
                let district = matches!(p.scope, MapScope::District { .. });
                let mut choices = vec![
                    (false, vec![ExportFormat::Png]),
                    (false, vec![ExportFormat::Svg]),
                    (false, vec![ExportFormat::Json]),
                    (
                        false,
                        vec![ExportFormat::Png, ExportFormat::Svg, ExportFormat::Json],
                    ),
                ];
                let mut items = vec![
                    PickItem::new(format!("{}: PNG picture", p.label)),
                    PickItem::new(format!("{}: SVG drawing", p.label)),
                    PickItem::new(format!("{}: map data (JSON)", p.label)),
                    PickItem::new(format!("{}: all three", p.label)),
                ];
                if district {
                    choices.push((true, vec![ExportFormat::Png]));
                    items.push(PickItem::new("Every drawn district: PNG, one file each"));
                    choices.push((true, vec![ExportFormat::Svg]));
                    items.push(PickItem::new("Every drawn district: SVG, one file each"));
                }
                self.modal = Some((Modal::pick("Export", items, 0), Pending::Export(choices)));
            }
            Action::Rename => {
                if let MapScope::District { district } = &p.scope {
                    self.modal = Some((
                        Modal::text(
                            "Rename district",
                            &format!(
                                "Leave empty for “{}”. Its map and records are unchanged.",
                                p.default_label.as_deref().unwrap_or(&p.label)
                            ),
                            if p.default_label.is_some() {
                                &p.label
                            } else {
                                ""
                            },
                        ),
                        Pending::RenameDistrict(district.id.clone()),
                    ));
                }
            }
            Action::DeleteMap => {
                let a = p.annotations;
                self.modal = Some((
                    Modal::confirm(
                        &format!(
                            "Delete the map of {}? {} building records and {} points of interest \
                             go with it; {} placed locations are taken off and kept.",
                            p.label, a.building_details, a.pois, a.locations
                        ),
                        "Delete map",
                    ),
                    Pending::DeleteMap,
                ));
            }
        }
        Nav::Stay
    }

    /// Handles a finished modal.
    fn finish(&mut self, pending: Pending, result: ModalResult, ctx: &mut Ctx) -> Nav {
        let Some(p) = self.current().cloned() else {
            return Nav::Stay;
        };
        let id = self.id.clone();
        match (pending, result) {
            (Pending::Redraw, ModalResult::Yes) => self.generate(ctx),
            (Pending::Building(ids), ModalResult::Picked(i)) => {
                return match BuildingScreen::new(ctx, &id, &p.id, &ids[i]) {
                    Ok(screen) => Nav::Push(Box::new(screen)),
                    Err(e) => {
                        self.error = Some(e);
                        Nav::Stay
                    }
                };
            }
            (Pending::Location(ids), ModalResult::Picked(i)) => {
                let lid = ids[i].clone();
                let placed = self
                    .view
                    .as_ref()
                    .and_then(|v| v.locations.iter().find(|l| l.id == lid))
                    .is_some_and(|l| l.placement.is_some());
                let mut acts = vec!["Place on a building of this map…", "Rename…"];
                if placed {
                    acts.insert(1, "Unassign");
                }
                let items = acts.iter().map(|a| PickItem::new(*a)).collect();
                self.modal = Some((
                    Modal::pick("Location", items, 0),
                    Pending::LocationAction(lid, acts),
                ));
            }
            (Pending::LocationAction(lid, acts), ModalResult::Picked(i)) => match acts[i] {
                "Unassign" => {
                    let result = ctx.service.maps_location_unassign(&id, &lid);
                    self.apply(
                        ctx,
                        result,
                        "Unassigned; the location and its number are kept",
                    );
                }
                "Rename…" => {
                    let (name, version) = self
                        .view
                        .as_ref()
                        .and_then(|v| v.locations.iter().find(|l| l.id == lid))
                        .map(|l| {
                            (
                                if l.renamed {
                                    l.name.clone()
                                } else {
                                    String::new()
                                },
                                l.version,
                            )
                        })
                        .unwrap_or_default();
                    self.modal = Some((
                        Modal::text(
                            "Rename location",
                            "Leave empty for its default name.",
                            &name,
                        ),
                        Pending::RenameLocation(lid, version),
                    ));
                }
                _ => {
                    let Some(map) = self.map(&p.id) else {
                        return Nav::Stay;
                    };
                    let items = map
                        .geometry
                        .buildings
                        .iter()
                        .map(|b| PickItem::new(building_label(&b.id)).hint(b.id.to_string()))
                        .collect();
                    let ids = map
                        .geometry
                        .buildings
                        .iter()
                        .map(|b| b.id.clone())
                        .collect();
                    self.modal = Some((
                        Modal::pick("Place on which building? (type to search)", items, 0),
                        Pending::Place(lid, ids),
                    ));
                }
            },
            (Pending::Place(lid, ids), ModalResult::Picked(i)) => {
                let target = PlaceOn {
                    map_id: p.id.clone(),
                    building_id: ids[i].clone(),
                };
                self.place(ctx, lid, target, AssignOptions::default());
            }
            (Pending::Consent(lid, target, options), ModalResult::Yes) => {
                self.place(ctx, lid, target, options);
            }
            (Pending::RenameLocation(lid, version), ModalResult::Text(name)) => {
                let edit = LocationEdit {
                    name: Some(name),
                    ..Default::default()
                };
                let result = ctx.service.maps_location_update(&id, &lid, version, edit);
                self.apply(ctx, result, "Location renamed");
            }
            (Pending::Point(ids), ModalResult::Picked(i)) => match &ids[i] {
                None => {
                    self.modal = Some((
                        Modal::text(
                            "New point of interest",
                            "Position: x y in metres from the map's centre (x east, y south).",
                            "0 0",
                        ),
                        Pending::PointAt(None),
                    ));
                }
                Some(poi) => {
                    let acts = vec!["Rename…", "Move…", "Delete"];
                    let items = acts.iter().map(|a| PickItem::new(*a)).collect();
                    self.modal = Some((
                        Modal::pick("Point of interest", items, 0),
                        Pending::PointAction(poi.clone(), acts),
                    ));
                }
            },
            (Pending::PointAction(poi, acts), ModalResult::Picked(i)) => {
                let current = self
                    .view
                    .as_ref()
                    .and_then(|v| v.pois.iter().find(|x| x.id.as_str() == poi))
                    .cloned();
                match acts[i] {
                    "Rename…" => {
                        let label = current.map(|c| c.label).unwrap_or_default();
                        self.modal = Some((
                            Modal::text("Rename point", "Its number stays.", &label),
                            Pending::RenamePoint(poi),
                        ));
                    }
                    "Move…" => {
                        let at = current.map(|c| point_text(c.position)).unwrap_or_default();
                        self.modal = Some((
                            Modal::text("Move point", "New position: x y in metres.", &at),
                            Pending::PointAt(Some(poi)),
                        ));
                    }
                    _ => {
                        self.modal = Some((
                            Modal::confirm(
                                "Delete this point? Its number is not reused.",
                                "Delete",
                            ),
                            Pending::DeletePoint(poi),
                        ));
                    }
                }
            }
            (Pending::PointAt(poi), ModalResult::Text(text)) => match parse_point(&text) {
                Err(e) => {
                    let mut modal = Modal::text("Position", "x y in metres", &text);
                    modal.set_error(e);
                    self.modal = Some((modal, Pending::PointAt(poi)));
                }
                Ok(at) => match poi {
                    Some(poi) => {
                        let result = ctx.service.maps_poi_move(&id, &poi, at);
                        self.apply(ctx, result, "Point moved");
                    }
                    None => {
                        self.modal = Some((
                            Modal::text("Name the point", "e.g. Old well", ""),
                            Pending::PointName(at),
                        ));
                    }
                },
            },
            (Pending::PointName(at), ModalResult::Text(label)) => {
                let result = ctx
                    .service
                    .maps_poi_add(&id, &p.id, at, label, String::new());
                self.apply(ctx, result, "Point of interest added");
            }
            (Pending::RenamePoint(poi), ModalResult::Text(label)) => {
                let notes = self
                    .view
                    .as_ref()
                    .and_then(|v| v.pois.iter().find(|x| x.id.as_str() == poi))
                    .map(|x| x.notes.clone())
                    .unwrap_or_default();
                let result = ctx.service.maps_poi_update(&id, &poi, label, notes);
                self.apply(ctx, result, "Point renamed");
            }
            (Pending::DeletePoint(poi), ModalResult::Yes) => {
                let result = ctx.service.maps_poi_delete(&id, &poi);
                self.apply(ctx, result, "Point deleted");
            }
            (Pending::Export(choices), ModalResult::Picked(i)) => {
                let (all, formats) = choices[i].clone();
                let request = MapExportRequest {
                    map: (!all).then(|| p.id.clone()),
                    formats,
                    options: RenderOptions::default(),
                    dir: ctx.export_dir.clone(),
                };
                match ctx.service.maps_export(&id, &request) {
                    Ok(r) => {
                        self.error = None;
                        ctx.flash = Some(format!(
                            "Saved {} file{} in {}",
                            r.files.len(),
                            if r.files.len() == 1 { "" } else { "s" },
                            r.dir.display()
                        ));
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            (Pending::RenameDistrict(district), ModalResult::Text(name)) => {
                let result = ctx.service.maps_district_rename(&id, &district, Some(name));
                self.apply(ctx, result, "District renamed");
            }
            (Pending::DeleteMap, ModalResult::Yes) => {
                let result = ctx.service.maps_delete_map(&id, &p.id);
                self.apply(ctx, result, "Map deleted; the district is kept");
                self.in_actions = false;
            }
            _ => {}
        }
        Nav::Stay
    }

    /// Places a location; a move or replace is asked about first.
    fn place(&mut self, ctx: &mut Ctx, lid: String, target: PlaceOn, options: AssignOptions) {
        match ctx
            .service
            .maps_location_assign(&self.id, &lid, &target, options)
        {
            Ok(v) => {
                self.view = Some(v);
                self.error = None;
                ctx.flash = Some("Notable Location placed".into());
            }
            Err(e) => match consent(&e) {
                Some(more) if options == AssignOptions::default() => {
                    self.modal = Some((
                        Modal::confirm(
                            &e,
                            if more.move_location {
                                "Move it"
                            } else {
                                "Replace"
                            },
                        ),
                        Pending::Consent(lid, target, more),
                    ));
                }
                _ => self.error = Some(e),
            },
        }
    }

    fn render_detail(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let mut lines: Vec<Line> = vec![];
        let Some(view) = &self.view else {
            f.render_widget(Paragraph::new(Line::styled("Loading…", s.dim())), area);
            return;
        };
        let Some(p) = self.current().cloned() else {
            let text = view
                .problem
                .clone()
                .unwrap_or_else(|| "Nothing to show.".into());
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
            return;
        };
        lines.push(Line::styled(p.label.clone(), s.title()));
        if let Some(d) = &p.default_label {
            lines.push(Line::styled(format!("renamed from {d}"), s.dim()));
        }
        let mut spans = Vec::new();
        for (i, action) in self.actions().iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(if self.in_actions && i == self.action {
                Span::styled(format!("[ {} ]", action.label()), s.selected())
            } else {
                Span::styled(format!("  {}  ", action.label()), s.dim())
            });
        }
        lines.push(Line::from(spans));
        if let Some(e) = &self.error {
            lines.push(Line::styled(e.clone(), s.error()));
        }
        for d in &view.damaged {
            lines.push(Line::styled(
                format!(
                    "Damaged map data not shown ({:?} {}): {}",
                    d.kind, d.id, d.problem
                ),
                s.error(),
            ));
        }
        if let Some(problem) = &view.problem {
            lines.push(Line::styled(problem.clone(), s.error()));
        }
        lines.push(Line::default());
        match self.map(&p.id) {
            None => lines.push(Line::styled(
                "No map yet. Generate draws it from the saved settlement; nothing about the \
                 settlement changes.",
                s.dim(),
            )),
            Some(map) => {
                let a = p.annotations;
                lines.push(Line::raw(format!(
                    "{} buildings ({} with details) · {} placed locations · {} points",
                    map.geometry.buildings.len(),
                    a.building_details,
                    a.locations,
                    a.pois
                )));
                lines.push(Line::styled(
                    format!(
                        "{} {} · seed {} · revision {} · {:.0} × {:.0} m",
                        map.engine.id,
                        map.engine.version,
                        map.seed,
                        map.revision,
                        map.geometry.bounds.width(),
                        map.geometry.bounds.height()
                    ),
                    s.dim(),
                ));
                if p.stale {
                    lines.push(Line::styled(
                        "The settlement changed after this map was drawn; it is kept as it was.",
                        s.dim(),
                    ));
                }
                lines.push(Line::default());
                lines.push(Line::styled("Legend", s.heading()));
                let legend = view.legends.iter().find(|l| l.map_id == p.id);
                match legend {
                    Some(l) if !l.entries.is_empty() => {
                        for e in &l.entries {
                            let (kind, at) = match &e.target {
                                MarkerTarget::NotableLocation { building_id, .. } => (
                                    e.location_type.clone().unwrap_or_default(),
                                    building_label(building_id),
                                ),
                                MarkerTarget::PointOfInterest { .. } => (
                                    "Point of interest".into(),
                                    l.markers
                                        .iter()
                                        .find(|m| m.number == e.number)
                                        .map(|m| format!("at {}", point_text(m.position)))
                                        .unwrap_or_default(),
                                ),
                            };
                            lines.push(Line::from(vec![
                                Span::styled(format!("{:>3}  ", e.number), s.key()),
                                Span::raw(e.label.clone()),
                                // The type, unless the name already says it.
                                Span::styled(
                                    if kind == e.label {
                                        format!("  {at}")
                                    } else {
                                        format!("  {kind} · {at}")
                                    },
                                    s.dim(),
                                ),
                            ]));
                        }
                    }
                    _ => lines.push(Line::styled("Nothing is marked on this map yet.", s.dim())),
                }
            }
        }
        let overflow = (lines.len() as u16).saturating_sub(area.height);
        self.scroll = self.scroll.min(overflow);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            area,
        );
    }
}

impl Screen for MapsScreen {
    fn title(&self) -> String {
        format!("Maps · {}", self.name)
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let rows = self.rows();
        if area.width >= 80 {
            let [left, sep, right] = Layout::horizontal([
                Constraint::Percentage(30),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .areas(area);
            self.state.render(f, left, &rows, s, !self.in_actions);
            let bar: Vec<Line> = (0..sep.height)
                .map(|_| Line::styled(" │", s.border()))
                .collect();
            f.render_widget(Paragraph::new(bar), sep);
            self.render_detail(f, right, ctx);
        } else {
            let height = (rows.len() as u16 + 1).min(area.height / 3).max(3);
            let [top, bottom] =
                Layout::vertical([Constraint::Length(height), Constraint::Fill(1)]).areas(area);
            self.state.render(f, top, &rows, s, !self.in_actions);
            self.render_detail(f, bottom, ctx);
        }
        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if matches!(result, ModalResult::Open) {
                return Nav::Stay;
            }
            let (_, pending) = self.modal.take().expect("modal is open");
            return self.finish(pending, result, ctx);
        }
        if self.in_actions {
            let actions = self.actions();
            match key.code {
                KeyCode::Right | KeyCode::Tab => {
                    self.action = (self.action + 1).min(actions.len().saturating_sub(1));
                }
                KeyCode::Left if self.action > 0 => self.action -= 1,
                KeyCode::Left | KeyCode::Esc | KeyCode::Backspace => self.in_actions = false,
                KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(&a) = actions.get(self.action) {
                        return self.run(a, ctx);
                    }
                }
                _ => {}
            }
            return Nav::Stay;
        }
        let rows = self.rows();
        if self.state.handle_nav(&rows, key) {
            self.error = None;
            self.scroll = 0;
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Right | KeyCode::Enter if !self.actions().is_empty() => {
                self.in_actions = true;
                self.action = 0;
                Nav::Stay
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => Nav::Pop,
            _ => Nav::Stay,
        }
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.in_actions {
            vec![
                ("←→", "action"),
                ("↑↓", "scroll"),
                ("Enter", "do"),
                ("Esc", "back to maps"),
            ]
        } else {
            vec![("↑↓", "map"), ("→", "actions"), ("Esc", "back")]
        }
    }

    fn resume(&mut self, ctx: &mut Ctx) {
        self.reload(ctx);
    }
}

// ----- one building -----------------------------------------------------------------

const FIELDS: [(&str, &str); 6] = [
    ("name", "Name or address"),
    ("building_type", "Type"),
    ("condition", "Condition"),
    ("occupancy", "Occupancy"),
    ("occupants", "Occupants"),
    ("description", "Description"),
];

fn field(d: &BuildingDetailsView, key: &str) -> (Option<String>, FieldOrigin, Option<String>) {
    fn text(f: &BuildingField<String>) -> (Option<String>, FieldOrigin, Option<String>) {
        (f.value.clone(), f.origin, f.generated.value.clone())
    }
    match key {
        "name" => text(&d.name),
        "building_type" => text(&d.building_type),
        "condition" => text(&d.condition),
        "occupancy" => text(&d.occupancy),
        "description" => text(&d.description),
        _ => (
            d.occupants.value.map(|n| n.to_string()),
            d.occupants.origin,
            d.occupants.generated.value.map(|n| n.to_string()),
        ),
    }
}

/// The user's current values, as the core takes them.
fn edits_of(d: &BuildingDetailsView) -> BuildingEdits {
    let user = |f: &BuildingField<String>| {
        (f.origin == FieldOrigin::User).then(|| UserValue::new(f.value.clone()))
    };
    BuildingEdits {
        name: user(&d.name),
        building_type: user(&d.building_type),
        description: user(&d.description),
        condition: user(&d.condition),
        occupancy: user(&d.occupancy),
        occupants: (d.occupants.origin == FieldOrigin::User)
            .then(|| UserValue::new(d.occupants.value)),
        notes: d.notes.clone(),
    }
}

/// `edits` with `key` set from `text`: `=` gives back the generated value,
/// empty leaves it blank on purpose.
fn with_field(mut edits: BuildingEdits, key: &str, text: &str) -> Result<BuildingEdits, String> {
    let text = text.trim();
    let generated = text == "=";
    let value = (!text.is_empty()).then(|| text.to_owned());
    let set = |slot: &mut Option<UserValue<String>>| {
        *slot = (!generated).then(|| UserValue::new(value.clone()));
    };
    match key {
        "name" => set(&mut edits.name),
        "building_type" => set(&mut edits.building_type),
        "condition" => set(&mut edits.condition),
        "occupancy" => set(&mut edits.occupancy),
        "description" => set(&mut edits.description),
        "notes" => edits.notes = text.to_owned(),
        _ => {
            edits.occupants = if generated {
                None
            } else {
                let n = value
                    .map(|v| {
                        v.parse::<u32>()
                            .map_err(|_| "Enter a whole number, or leave it empty.".to_string())
                    })
                    .transpose()?;
                Some(UserValue::new(n))
            };
        }
    }
    Ok(edits)
}

enum BuildingPending {
    Field(Vec<&'static str>),
    Edit(&'static str),
    Regenerate,
    RegenerateConfirm(bool),
    Reset,
    Notable(Vec<String>),
    Consent(String, AssignOptions),
}

/// One building: its details (generated the first time it is opened) and
/// the actions on them.
pub struct BuildingScreen {
    id: SettlementId,
    map: MapId,
    details: BuildingDetailsView,
    error: Option<String>,
    action: usize,
    modal: Option<(Modal, BuildingPending)>,
}

const BUILDING_ACTIONS: [&str; 5] = [
    "Edit a field…",
    "Make Notable Location…",
    "Regenerate…",
    "Reset…",
    "Back",
];

impl BuildingScreen {
    pub fn new(
        ctx: &mut Ctx,
        id: &SettlementId,
        map: &MapId,
        building: &BuildingId,
    ) -> Result<Self, String> {
        let details = ctx.service.maps_building(id, map, building)?;
        Ok(BuildingScreen {
            id: id.clone(),
            map: map.clone(),
            details,
            error: None,
            action: 0,
            modal: None,
        })
    }

    fn take(&mut self, ctx: &mut Ctx, result: Result<BuildingDetailsView, String>, done: &str) {
        match result {
            Ok(d) => {
                self.details = d;
                self.error = None;
                ctx.flash = Some(done.into());
            }
            Err(e) => {
                self.error = Some(e);
                if let Ok(d) =
                    ctx.service
                        .maps_building(&self.id, &self.map, &self.details.building_id)
                {
                    self.details = d;
                }
            }
        }
    }

    fn run(&mut self, ctx: &mut Ctx) -> Nav {
        let d = &self.details;
        match BUILDING_ACTIONS[self.action] {
            "Edit a field…" => {
                let mut keys: Vec<&'static str> = FIELDS.iter().map(|(k, _)| *k).collect();
                keys.push("notes");
                let items = keys
                    .iter()
                    .map(|k| {
                        let label = FIELDS
                            .iter()
                            .find(|(f, _)| f == k)
                            .map_or("Notes", |(_, l)| *l);
                        let value = if *k == "notes" {
                            d.notes.clone()
                        } else {
                            field(d, k).0.unwrap_or_else(|| "—".into())
                        };
                        PickItem::new(label).hint(value)
                    })
                    .collect();
                self.modal = Some((
                    Modal::pick("Edit which field?", items, 0),
                    BuildingPending::Field(keys),
                ));
            }
            "Make Notable Location…" => match ctx.service.maps_get(&self.id) {
                Ok(view) => {
                    let ids = view.locations.iter().map(|l| l.id.clone()).collect();
                    let items = view
                        .locations
                        .iter()
                        .map(|l| {
                            PickItem::new(l.name.clone()).hint(format!(
                                "{} · {}",
                                l.location_type,
                                location_status(l)
                            ))
                        })
                        .collect();
                    self.modal = Some((
                        Modal::pick("Which Notable Location stands here?", items, 0),
                        BuildingPending::Notable(ids),
                    ));
                }
                Err(e) => self.error = Some(e),
            },
            "Regenerate…" => {
                let items = vec![
                    PickItem::new("Reroll generated fields, keep my changes"),
                    PickItem::new("Reroll everything, replacing my changes"),
                ];
                self.modal = Some((
                    Modal::pick("Regenerate details", items, 0),
                    BuildingPending::Regenerate,
                ));
            }
            "Reset…" => {
                self.modal = Some((
                    Modal::confirm(
                        "Give back the generated values of the fields you changed? Notes are kept.",
                        "Reset",
                    ),
                    BuildingPending::Reset,
                ));
            }
            _ => return Nav::Pop,
        }
        Nav::Stay
    }

    fn finish(&mut self, pending: BuildingPending, result: ModalResult, ctx: &mut Ctx) {
        let (id, map, b, version) = (
            self.id.clone(),
            self.map.clone(),
            self.details.building_id.clone(),
            self.details.version,
        );
        match (pending, result) {
            (BuildingPending::Field(keys), ModalResult::Picked(i)) => {
                let key = keys[i];
                let (value, _, generated) = if key == "notes" {
                    (Some(self.details.notes.clone()), FieldOrigin::User, None)
                } else {
                    field(&self.details, key)
                };
                let help = match generated {
                    Some(g) => format!("Empty: blank on purpose. = : the generated value ({g})."),
                    None => "Empty: blank on purpose. = : the generated value.".into(),
                };
                self.modal = Some((
                    Modal::text("New value", &help, &value.unwrap_or_default()),
                    BuildingPending::Edit(key),
                ));
            }
            (BuildingPending::Edit(key), ModalResult::Text(text)) => {
                match with_field(edits_of(&self.details), key, &text) {
                    Ok(edits) => {
                        let result = ctx
                            .service
                            .maps_building_edit(&id, &map, &b, version, edits);
                        self.take(ctx, result, "Saved");
                    }
                    Err(e) => {
                        let mut modal = Modal::text("New value", "", &text);
                        modal.set_error(e);
                        self.modal = Some((modal, BuildingPending::Edit(key)));
                    }
                }
            }
            (BuildingPending::Regenerate, ModalResult::Picked(i)) => {
                let discard = i == 1;
                self.modal = Some((
                    Modal::confirm(
                        if discard {
                            "Reroll every generated field and replace your changes? Notes are kept."
                        } else {
                            "Reroll the generated fields? Your changes and notes are kept."
                        },
                        "Regenerate",
                    ),
                    BuildingPending::RegenerateConfirm(discard),
                ));
            }
            (BuildingPending::RegenerateConfirm(discard), ModalResult::Yes) => {
                let result = ctx
                    .service
                    .maps_building_regenerate(&id, &map, &b, version, discard);
                self.take(ctx, result, "Details regenerated");
            }
            (BuildingPending::Reset, ModalResult::Yes) => {
                let result = ctx.service.maps_building_reset(&id, &map, &b, version);
                self.take(ctx, result, "Generated values restored");
            }
            (BuildingPending::Notable(ids), ModalResult::Picked(i)) => {
                self.place(ctx, ids[i].clone(), AssignOptions::default());
            }
            (BuildingPending::Consent(lid, options), ModalResult::Yes) => {
                self.place(ctx, lid, options)
            }
            _ => {}
        }
    }

    fn place(&mut self, ctx: &mut Ctx, lid: String, options: AssignOptions) {
        let target = PlaceOn {
            map_id: self.map.clone(),
            building_id: self.details.building_id.clone(),
        };
        match ctx
            .service
            .maps_location_assign(&self.id, &lid, &target, options)
        {
            Ok(_) => {
                self.error = None;
                ctx.flash = Some("Notable Location placed on this building".into());
            }
            Err(e) => match consent(&e) {
                Some(more) if options == AssignOptions::default() => {
                    self.modal = Some((
                        Modal::confirm(
                            &e,
                            if more.move_location {
                                "Move it"
                            } else {
                                "Replace"
                            },
                        ),
                        BuildingPending::Consent(lid, more),
                    ));
                }
                _ => self.error = Some(e),
            },
        }
    }
}

impl Screen for BuildingScreen {
    fn title(&self) -> String {
        format!(
            "{} · {}",
            building_label(&self.details.building_id),
            self.details.map_label
        )
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let s = &ctx.styles;
        let d = &self.details;
        let state = format!("{:?}", d.state).to_lowercase();
        let mut lines = vec![
            Line::styled(
                d.name
                    .value
                    .clone()
                    .unwrap_or_else(|| building_label(&d.building_id)),
                s.title(),
            ),
            Line::styled(
                format!("{} · {state} · version {}", d.building_id, d.version),
                s.dim(),
            ),
            Line::default(),
        ];
        let mut spans = Vec::new();
        for (i, a) in BUILDING_ACTIONS.iter().enumerate() {
            spans.push(if i == self.action {
                Span::styled(format!("[ {a} ]"), s.selected())
            } else {
                Span::styled(format!("  {a}  "), s.dim())
            });
        }
        lines.push(Line::from(spans));
        if let Some(e) = &self.error {
            lines.push(Line::styled(e.clone(), s.error()));
        }
        lines.push(Line::default());
        for (key, label) in FIELDS {
            let (value, origin, _) = field(d, key);
            let origin = match origin {
                FieldOrigin::Table => "rolled",
                FieldOrigin::Inferred => "inferred",
                FieldOrigin::NotRolled => "not rolled",
                FieldOrigin::User => "edited",
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{label:<16}"), s.dim()),
                Span::raw(value.unwrap_or_else(|| "—".into())),
                Span::styled(format!("  ({origin})"), s.dim()),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{:<16}", "Notes"), s.dim()),
            Span::raw(if d.notes.is_empty() {
                "—".into()
            } else {
                d.notes.clone()
            }),
        ]));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        if let Some((modal, _)) = &mut self.modal {
            modal.render(f, area, s);
        }
    }

    fn handle(&mut self, key: KeyEvent, ctx: &mut Ctx) -> Nav {
        if let Some((modal, _)) = &mut self.modal {
            let result = modal.handle(key, &ctx.styles);
            if matches!(result, ModalResult::Open) {
                return Nav::Stay;
            }
            let (_, pending) = self.modal.take().expect("modal is open");
            self.finish(pending, result, ctx);
            return Nav::Stay;
        }
        match key.code {
            KeyCode::Right | KeyCode::Tab => {
                self.action = (self.action + 1).min(BUILDING_ACTIONS.len() - 1);
            }
            KeyCode::Left => self.action = self.action.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char(' ') => return self.run(ctx),
            KeyCode::Esc | KeyCode::Backspace => return Nav::Pop,
            _ => {}
        }
        Nav::Stay
    }

    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![("←→", "action"), ("Enter", "do"), ("Esc", "back")]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_read_as_two_numbers() {
        assert_eq!(parse_point("12 -30").unwrap(), Point(12.0, -30.0));
        assert_eq!(parse_point(" 1.5, 2 ").unwrap(), Point(1.5, 2.0));
        assert!(parse_point("12").is_err());
        assert!(parse_point("a b").is_err());
        assert!(parse_point("1 2 3").is_err());
    }

    #[test]
    fn field_text_follows_the_conventions() {
        let none = BuildingEdits::default();
        let e = with_field(none.clone(), "name", "  The Bell ").unwrap();
        assert_eq!(e.name, Some(UserValue::new(Some("The Bell".into()))));
        let e = with_field(none.clone(), "name", "").unwrap();
        assert_eq!(e.name, Some(UserValue::new(None)), "blank on purpose");
        let e = with_field(e, "name", "=").unwrap();
        assert_eq!(e.name, None, "back to generated");
        assert!(with_field(none.clone(), "occupants", "many").is_err());
        let e = with_field(none, "occupants", "4").unwrap();
        assert_eq!(e.occupants, Some(UserValue::new(Some(4))));
    }
}
