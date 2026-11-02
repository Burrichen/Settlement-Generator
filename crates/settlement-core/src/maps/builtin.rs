//! The built-in map engine: an original, deterministic street-and-lot
//! planner written for this application (it is not derived from watabou's
//! generator or from settlemaker). See `docs/MAPS.md` for what it draws and
//! what it does not.
//!
//! Outline:
//!
//! 1. Water (coast, river or lake), then the roads leaving the map, bent away
//!    from coast and lake. A settlement with fewer than two roads gets an
//!    internal street so it has a high street.
//! 2. An open square at the centre, if asked for.
//! 3. Growth outward from the centre in 10 m bands. Lots are offered along
//!    every street's frontage and accepted nearest first, when they stay
//!    clear of water, streets, the square and each other. Ring streets (with
//!    spokes between them) are added as the growth reaches them, for
//!    settlements too large or too planned for the roads alone.
//! 4. Walls around the built area (split where they would stand in water),
//!    with gates where roads cross them; fields and trees outside it.

use std::collections::HashMap;
use std::f64::consts::TAU;

use super::engine::{MapEngineAdapter, MapEngineError, map_key};
use super::geometry::{self, Bounds, Point};
use super::model::{
    AreaFeature, BuildingSize, EngineInfo, GEOMETRY_VERSION, MapBuilding, MapFeature,
    MapGenerationRequest, MapGenerationResult, MapGeometry, MapParameters, MapStats, Street,
    StreetKind, Terrain, TreeCluster, Wall, WaterFeature, WaterKind,
};
use super::rng::MapRng;
use crate::ids::BuildingId;

pub const BUILTIN_ENGINE_ID: &str = "builtin.medieval";
/// Bump whenever the same request would draw different geometry.
pub const BUILTIN_ENGINE_VERSION: &str = "1.0.0";

/// Growth step outward from the centre.
const BAND_M: f64 = 10.0;
const CELL_M: f64 = 25.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinEngine;

impl MapEngineAdapter for BuiltinEngine {
    fn info(&self) -> EngineInfo {
        use MapFeature::*;
        EngineInfo {
            id: BUILTIN_ENGINE_ID.into(),
            version: BUILTIN_ENGINE_VERSION.into(),
            features: vec![
                Buildings, Streets, Walls, Gates, Coast, River, Lake, Bridges, Trees, Fields,
                Squares,
            ],
        }
    }

    fn generate(
        &self,
        request: &MapGenerationRequest,
    ) -> Result<MapGenerationResult, MapEngineError> {
        let mut planner = Planner::new(request);
        planner.run();
        let geometry = planner.finish();
        if geometry.buildings.is_empty() {
            return Err(MapEngineError::Failed(
                "no building fits within the map radius".into(),
            ));
        }
        Ok(MapGenerationResult {
            engine: self.info(),
            geometry_version: GEOMETRY_VERSION,
            stats: MapStats {
                buildings_requested: request.parameters.building_count,
                buildings_placed: geometry.buildings.len() as u32,
            },
            geometry,
        })
    }
}

struct Candidate {
    footprint: Vec<Point>,
    centre: Point,
    distance: f64,
    priority: f64,
    size: BuildingSize,
    street: usize,
}

struct Placed {
    footprint: Vec<Point>,
    /// Footprint grown by the gap kept between buildings.
    clearance: Vec<Point>,
    size: BuildingSize,
    street: Option<usize>,
}

/// A street segment with its half width, for clearance checks.
#[derive(Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
    half: f64,
}

#[derive(Default)]
struct Grid(HashMap<(i32, i32), Vec<usize>>);

impl Grid {
    fn cell(p: Point) -> (i32, i32) {
        ((p.0 / CELL_M).floor() as i32, (p.1 / CELL_M).floor() as i32)
    }

    fn insert_box(&mut self, lo: Point, hi: Point, item: usize) {
        let (x0, y0) = Self::cell(lo);
        let (x1, y1) = Self::cell(hi);
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.0.entry((x, y)).or_default().push(item);
            }
        }
    }

    /// Items in the cells overlapping the box (may repeat).
    fn near(&self, lo: Point, hi: Point) -> Vec<usize> {
        let (x0, y0) = Self::cell(lo);
        let (x1, y1) = Self::cell(hi);
        let mut out = Vec::new();
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(items) = self.0.get(&(x, y)) {
                    out.extend_from_slice(items);
                }
            }
        }
        out
    }
}

fn bbox(points: &[Point]) -> (Point, Point) {
    points.iter().fold(
        (Point(f64::MAX, f64::MAX), Point(f64::MIN, f64::MIN)),
        |(lo, hi), p| {
            (
                Point(lo.0.min(p.0), lo.1.min(p.1)),
                Point(hi.0.max(p.0), hi.1.max(p.1)),
            )
        },
    )
}

/// A smoothed random walk: successive offsets that wander without jumping.
fn walker(amplitude: f64) -> impl FnMut(&mut MapRng) -> f64 {
    let mut w = 0.0;
    move |rng: &mut MapRng| {
        w = w * 0.6 + rng.signed() * amplitude;
        w
    }
}

fn rotate(v: Point, angle: f64) -> Point {
    let (s, c) = angle.sin_cos();
    Point(v.0 * c - v.1 * s, v.0 * s + v.1 * c)
}

struct Planner<'r> {
    req: &'r MapGenerationRequest,
    p: &'r MapParameters,
    rng: MapRng,
    bounds: Bounds,
    water: Vec<WaterFeature>,
    streets: Vec<Street>,
    segments: Vec<Segment>,
    segment_grid: Grid,
    square: Option<Vec<Point>>,
    placed: Vec<Placed>,
    building_grid: Grid,
    pending: Vec<Candidate>,
    walls: Vec<Wall>,
    fields: Vec<AreaFeature>,
    trees: Vec<TreeCluster>,
}

impl<'r> Planner<'r> {
    fn new(req: &'r MapGenerationRequest) -> Self {
        let p = &req.parameters;
        let half = (p.radius_m * 1.45 + 30.0).ceil();
        Planner {
            req,
            p,
            rng: MapRng::new(req.seed),
            bounds: Bounds::square(half),
            water: Vec::new(),
            streets: Vec::new(),
            segments: Vec::new(),
            segment_grid: Grid::default(),
            square: None,
            placed: Vec::new(),
            building_grid: Grid::default(),
            pending: Vec::new(),
            walls: Vec::new(),
            fields: Vec::new(),
            trees: Vec::new(),
        }
    }

    fn target(&self) -> usize {
        self.p.building_count as usize
    }

    fn run(&mut self) {
        self.add_water();
        self.add_roads();
        if self.p.square {
            self.add_square();
        }
        self.grow();
        self.crop();
        if self.p.walls {
            self.add_walls();
        }
        self.add_bridges();
        if self.p.fields {
            self.add_fields();
        }
        self.add_trees();
    }

    // ----- water ------------------------------------------------------------

    fn in_water(&self, p: Point) -> bool {
        self.water.iter().any(|w| geometry::contains(&w.area, p))
    }

    /// In water the streets must avoid (coast and lake; rivers are bridged).
    fn in_still_water(&self, p: Point) -> bool {
        self.water
            .iter()
            .any(|w| w.kind != WaterKind::River && geometry::contains(&w.area, p))
    }

    fn add_water(&mut self) {
        let r = self.p.radius_m;
        let reach = self.bounds.max_x * 1.6;
        match self.p.water {
            WaterKind::None => {}
            WaterKind::Coast => {
                let n = Point::from_bearing(self.rng.range(0.0, TAU));
                let t = n.perpendicular();
                let d = r * self.rng.range(0.6, 0.9);
                let mut wobble = walker((0.02 + 0.08 * self.p.irregularity) * r);
                let steps = 32;
                let mut ring: Vec<Point> = (0..=steps)
                    .map(|i| {
                        let s = -reach + 2.0 * reach * f64::from(i) / f64::from(steps);
                        t.scale(s).plus(n.scale(d + wobble(&mut self.rng)))
                    })
                    .collect();
                let first = ring[0];
                let last = ring[ring.len() - 1];
                ring.push(last.plus(n.scale(2.0 * reach)));
                ring.push(first.plus(n.scale(2.0 * reach)));
                self.push_water(WaterKind::Coast, ring, Vec::new());
            }
            WaterKind::River => {
                let u = Point::from_bearing(self.rng.range(0.0, TAU));
                let n = u.perpendicular();
                let side = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                let offset = side * r * self.rng.range(0.3, 0.6);
                let width = self.rng.range(8.0, 16.0);
                let mut meander = walker((0.04 + 0.12 * self.p.irregularity) * r);
                let steps = 40;
                let line: Vec<Point> = (0..=steps)
                    .map(|i| {
                        let s = -reach + 2.0 * reach * f64::from(i) / f64::from(steps);
                        u.scale(s).plus(n.scale(offset + meander(&mut self.rng)))
                    })
                    .collect();
                let mut ring: Vec<Point> =
                    line.iter().map(|p| p.plus(n.scale(width / 2.0))).collect();
                ring.extend(line.iter().rev().map(|p| p.minus(n.scale(width / 2.0))));
                let bounds = self.bounds;
                let centreline = line.into_iter().filter(|p| bounds.contains(*p)).collect();
                self.push_water(WaterKind::River, ring, centreline);
            }
            WaterKind::Lake => {
                let c = Point::from_bearing(self.rng.range(0.0, TAU))
                    .scale(r * self.rng.range(0.6, 0.9));
                let rx = r * self.rng.range(0.2, 0.35);
                let ry = rx * self.rng.range(0.6, 1.0);
                let tilt = self.rng.range(0.0, TAU);
                let jitter = 0.05 + 0.15 * self.p.irregularity;
                let ring: Vec<Point> = (0..28)
                    .map(|i| {
                        let a = TAU * f64::from(i) / 28.0;
                        let k = 1.0 + jitter * self.rng.signed();
                        c.plus(rotate(Point(a.cos() * rx * k, a.sin() * ry * k), tilt))
                    })
                    .collect();
                self.push_water(WaterKind::Lake, ring, Vec::new());
            }
        }
    }

    fn push_water(&mut self, kind: WaterKind, ring: Vec<Point>, centreline: Vec<Point>) {
        let area = geometry::clip_to_bounds(&ring, &self.bounds);
        if area.len() >= 3 && geometry::area(&area) > 0.0 {
            self.water.push(WaterFeature {
                id: format!("water-{}", self.water.len() + 1),
                kind,
                area,
                centreline,
                bridges: Vec::new(),
            });
        }
    }

    // ----- streets ----------------------------------------------------------

    fn road_width(&self) -> f64 {
        if self.p.building_count >= 60 {
            8.0
        } else {
            6.0
        }
    }

    fn add_street(&mut self, kind: StreetKind, width: f64, path: Vec<Point>) -> Option<usize> {
        if path.len() < 2 {
            return None;
        }
        let index = self.streets.len();
        self.index_segments(&path, width);
        self.streets.push(Street {
            id: format!("street-{}", index + 1),
            kind,
            width_m: width,
            path,
        });
        self.offer_lots(index);
        Some(index)
    }

    /// Records a street's segments for clearance checks.
    fn index_segments(&mut self, path: &[Point], width: f64) {
        for pair in path.windows(2) {
            let seg = Segment {
                a: pair[0],
                b: pair[1],
                half: width / 2.0,
            };
            let (lo, hi) = bbox(&[seg.a, seg.b]);
            let pad = Point(seg.half, seg.half);
            self.segment_grid
                .insert_box(lo.minus(pad), hi.plus(pad), self.segments.len());
            self.segments.push(seg);
        }
    }

    /// A wandering line from the centre along `bearing`, stopping at
    /// `length` or the map edge.
    fn line_from_centre(&mut self, bearing: f64, length: f64) -> Vec<Point> {
        let u = Point::from_bearing(bearing);
        let n = u.perpendicular();
        let mut wobble = walker(1.0 + 5.0 * self.p.irregularity);
        let mut path = vec![Point(0.0, 0.0)];
        let mut s = 0.0;
        while s < length {
            s = (s + 20.0).min(length);
            let next = u.scale(s).plus(n.scale(wobble(&mut self.rng)));
            if !self.bounds.contains(next) {
                let last = *path.last().expect("path starts at the centre");
                if let Some(edge) = self.edge_crossing(last, next) {
                    path.push(edge);
                }
                break;
            }
            path.push(next);
        }
        path
    }

    fn edge_crossing(&self, a: Point, b: Point) -> Option<Point> {
        let c = self.bounds.corners();
        (0..4).find_map(|i| geometry::segment_intersection(a, b, c[i], c[(i + 1) % 4]))
    }

    /// Whether a road on `bearing` would run into the sea or a lake.
    fn bearing_blocked(&self, bearing: f64) -> bool {
        let u = Point::from_bearing(bearing);
        let r = self.p.radius_m;
        (1..=12).any(|i| self.in_still_water(u.scale(r * 0.15 * f64::from(i))))
    }

    fn add_roads(&mut self) {
        let n = self.p.approaches as usize;
        let start = self.rng.range(0.0, TAU);
        let mut bearings: Vec<f64> = Vec::new();
        for i in 0..n {
            let spread = TAU / n as f64;
            let ideal =
                start + spread * i as f64 + self.rng.signed() * spread * 0.3 * self.p.irregularity;
            // Turn away from water: +20°, −20°, +40°, −40° … up to ±180°.
            let turned = (0..=18).map(|k| {
                let step = f64::from((k + 1) / 2) * 20f64.to_radians();
                if k % 2 == 1 {
                    ideal + step
                } else {
                    ideal - step
                }
            });
            if let Some(b) = turned.into_iter().find(|b| !self.bearing_blocked(*b)) {
                bearings.push(b);
            }
        }
        let reach = self.bounds.max_x * 2.0;
        let width = self.road_width();
        for &b in &bearings {
            let path = self.line_from_centre(b, reach);
            self.add_street(StreetKind::Road, width, path);
        }
        // A high street, so even an isolated settlement has two arms.
        let arms_needed = 2usize.saturating_sub(bearings.len());
        let base = bearings.first().copied().unwrap_or(start);
        for k in 0..arms_needed {
            let b = base + std::f64::consts::PI * (1.0 + k as f64 * 0.5);
            if self.bearing_blocked(b) {
                continue;
            }
            let path = self.line_from_centre(b, self.p.radius_m);
            self.add_street(StreetKind::Street, 5.0, path);
        }
    }

    fn add_square(&mut self) {
        // Never more than a third of the map, however small.
        let rs = ((self.p.building_count as f64).sqrt() * 2.0)
            .clamp(12.0, 40.0)
            .min(self.p.radius_m * 0.3);
        let k = self.rng.int(5, 8);
        let jitter = 0.15 * self.p.irregularity;
        let turn = self.rng.range(0.0, TAU);
        let ring: Vec<Point> = (0..k)
            .map(|i| {
                let a = turn + TAU * f64::from(i) / f64::from(k);
                Point::from_bearing(a).scale(rs * (1.0 + jitter * self.rng.signed()))
            })
            .collect();
        // Lots offered before the square existed must respect it too.
        self.pending
            .retain(|c| !geometry::convex_overlap(&c.footprint, &ring));
        self.square = Some(ring);
    }

    /// Ring streets add frontage when the roads alone cannot hold the
    /// settlement, or when it is planned around a square.
    fn wants_rings(&self) -> bool {
        let arms = self.streets.len().max(1) as f64;
        let road_capacity = arms * 2.0 * self.p.radius_m / 9.0;
        self.p.square || self.target() as f64 > road_capacity * 0.8
    }

    fn add_ring(&mut self, radius: f64, previous: f64) {
        let points = 36;
        let mut wobble = walker(0.05 * self.p.irregularity);
        let ring: Vec<Point> = (0..points)
            .map(|i| {
                let a = TAU * f64::from(i) / f64::from(points);
                Point::from_bearing(a).scale(radius * (1.0 + wobble(&mut self.rng)))
            })
            .collect();
        // Split the closed ring where it would cross still water.
        let mut runs: Vec<Vec<Point>> = Vec::new();
        let mut run: Vec<Point> = Vec::new();
        for i in 0..=points as usize {
            let p = ring[i % points as usize];
            if self.in_still_water(p) || !self.bounds.contains(p) {
                if run.len() >= 2 {
                    runs.push(std::mem::take(&mut run));
                }
                run.clear();
            } else {
                run.push(p);
            }
        }
        if run.len() >= 2 {
            runs.push(run);
        }
        for path in runs {
            self.add_street(StreetKind::Street, 5.0, path);
        }
        // Spokes from the previous ring (or the square) out to this one.
        let spokes = ((radius * TAU / 110.0).round() as u32).max(2);
        let turn = self.rng.range(0.0, TAU);
        for i in 0..spokes {
            let a = turn + TAU * f64::from(i) / f64::from(spokes) + self.rng.signed() * 0.15;
            let u = Point::from_bearing(a);
            let path = vec![u.scale(previous), u.scale(radius)];
            if path.iter().any(|p| self.in_still_water(*p)) {
                continue;
            }
            self.add_street(StreetKind::Lane, 3.5, path);
        }
    }

    // ----- lots -------------------------------------------------------------

    /// A lot's size class and street frontage.
    fn lot_size(&mut self) -> (BuildingSize, f64) {
        let large = if self.p.square { 0.08 } else { 0.04 };
        let roll = self.rng.unit();
        if roll < large {
            (BuildingSize::Large, self.rng.range(12.0, 18.0))
        } else if roll < large + 0.4 {
            (BuildingSize::Medium, self.rng.range(7.5, 10.5))
        } else {
            (BuildingSize::Small, self.rng.range(5.0, 7.5))
        }
    }

    /// Offers lots along both sides of a street.
    fn offer_lots(&mut self, street: usize) {
        let path = self.streets[street].path.clone();
        let half = self.streets[street].width_m / 2.0;
        let irregular = self.p.irregularity;
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = a.distance(b);
            if length < 4.0 {
                continue;
            }
            let u = b.minus(a).normalised();
            let n = u.perpendicular();
            let mut t = self.rng.range(0.0, 3.0);
            loop {
                let (size, width) = self.lot_size();
                if t + width > length {
                    break;
                }
                for side in [1.0, -1.0] {
                    let depth = match size {
                        BuildingSize::Large => self.rng.range(11.0, 16.0),
                        BuildingSize::Medium => self.rng.range(7.5, 11.0),
                        BuildingSize::Small => self.rng.range(5.5, 8.0),
                    };
                    let setback = half + self.rng.range(0.6, 2.0);
                    let centre = a
                        .plus(u.scale(t + width / 2.0))
                        .plus(n.scale(side * (setback + depth / 2.0)));
                    let along = rotate(u, irregular * 0.12 * self.rng.signed());
                    let distance = centre.length();
                    self.pending.push(Candidate {
                        footprint: geometry::oriented_rect(centre, along, width, depth),
                        centre,
                        distance,
                        priority: distance
                            + self.rng.range(0.0, 2.0)
                            + self.rng.range(0.0, 12.0) * irregular,
                        size,
                        street,
                    });
                }
                t += width + self.rng.range(0.3, 1.8);
            }
        }
    }

    fn clear_of_streets(&self, footprint: &[Point]) -> bool {
        let (lo, hi) = bbox(footprint);
        let mut near = self.segment_grid.near(lo, hi);
        near.sort_unstable();
        near.dedup();
        let n = footprint.len();
        near.into_iter().all(|i| {
            let s = self.segments[i];
            footprint
                .iter()
                .all(|p| geometry::segment_distance(*p, s.a, s.b) >= s.half + 0.4)
                && (0..n).all(|k| {
                    geometry::segment_intersection(s.a, s.b, footprint[k], footprint[(k + 1) % n])
                        .is_none()
                })
                && !geometry::contains(footprint, s.a)
        })
    }

    fn fits(&self, c: &Candidate) -> bool {
        let r = self.p.radius_m;
        if c.footprint
            .iter()
            .any(|p| p.length() > r * 1.02 || !self.bounds.contains(*p))
        {
            return false;
        }
        if self.in_water(c.centre) || c.footprint.iter().any(|p| self.in_water(*p)) {
            return false;
        }
        if self
            .square
            .as_ref()
            .is_some_and(|square| geometry::convex_overlap(&c.footprint, square))
        {
            return false;
        }
        if !self.clear_of_streets(&c.footprint) {
            return false;
        }
        let (lo, hi) = bbox(&c.footprint);
        let pad = Point(1.0, 1.0);
        self.building_grid
            .near(lo.minus(pad), hi.plus(pad))
            .into_iter()
            .all(|i| !geometry::convex_overlap(&c.footprint, &self.placed[i].clearance))
    }

    fn place(&mut self, c: Candidate) {
        let along = c.footprint[1].minus(c.footprint[0]);
        let width = along.length();
        let depth = c.footprint[3].distance(c.footprint[0]);
        let clearance =
            geometry::oriented_rect(c.centre, along.normalised(), width + 1.0, depth + 1.0);
        let (lo, hi) = bbox(&clearance);
        self.building_grid.insert_box(lo, hi, self.placed.len());
        self.placed.push(Placed {
            footprint: c.footprint,
            clearance,
            size: c.size,
            street: Some(c.street),
        });
    }

    fn grow(&mut self) {
        let r = self.p.radius_m;
        let rings = self.wants_rings();
        // Two back-to-back rows of lots between neighbouring rings.
        let spacing = 25.0 + 6.0 * self.p.irregularity;
        let core = self
            .square
            .as_ref()
            .map_or(0.0, |s| s.iter().map(|p| p.length()).fold(0.0, f64::max));
        let mut next_ring = core + spacing * 0.9;
        let mut previous_ring = core;
        let mut limit = core + BAND_M;
        loop {
            while rings && next_ring <= limit + spacing * 0.5 && next_ring <= r {
                self.add_ring(next_ring, previous_ring);
                previous_ring = next_ring;
                next_ring += spacing;
            }
            let mut ready: Vec<Candidate> = Vec::new();
            let mut later: Vec<Candidate> = Vec::new();
            for c in self.pending.drain(..) {
                if c.distance <= limit {
                    ready.push(c)
                } else {
                    later.push(c)
                }
            }
            self.pending = later;
            ready.sort_by(|a, b| a.priority.total_cmp(&b.priority));
            for c in ready {
                if self.placed.len() >= self.target() {
                    break;
                }
                if self.fits(&c) {
                    self.place(c);
                }
            }
            if self.placed.len() >= self.target() || limit >= r * 1.02 + BAND_M {
                break;
            }
            limit += BAND_M;
        }
    }

    /// Shrinks the frame to the built area plus a margin of countryside, and
    /// re-clips water and streets to it, so the map shows the settlement
    /// rather than the empty radius it was allowed.
    fn crop(&mut self) {
        // Whole metres: edge points then round exactly onto the frame, and the
        // stored value reads back exactly.
        let half = (self.built_radius() * 1.35 + 30.0).max(45.0).ceil();
        if half >= self.bounds.max_x {
            return;
        }
        let b = Bounds::square(half);
        self.bounds = b;
        for w in &mut self.water {
            w.area = geometry::clip_to_bounds(&w.area, &b);
            w.centreline.retain(|p| b.contains(*p));
        }
        self.water
            .retain(|w| w.area.len() >= 3 && geometry::area(&w.area) > 0.0);
        for (i, w) in self.water.iter_mut().enumerate() {
            w.id = format!("water-{}", i + 1);
        }
        // Keep the part of each street that starts nearest the centre.
        let mut remap = vec![None; self.streets.len()];
        let mut kept = Vec::new();
        for (i, mut street) in std::mem::take(&mut self.streets).into_iter().enumerate() {
            if let Some(path) = geometry::clip_polyline(&street.path, &b).into_iter().next() {
                street.path = path;
                street.id = format!("street-{}", kept.len() + 1);
                remap[i] = Some(kept.len());
                kept.push(street);
            }
        }
        self.streets = kept;
        for p in &mut self.placed {
            p.street = p.street.and_then(|s| remap[s]);
        }
        self.segments.clear();
        self.segment_grid = Grid::default();
        for i in 0..self.streets.len() {
            let (path, width) = (self.streets[i].path.clone(), self.streets[i].width_m);
            self.index_segments(&path, width);
        }
    }

    // ----- walls, bridges, fields, trees ------------------------------------

    fn built_radius(&self) -> f64 {
        self.placed
            .iter()
            .flat_map(|b| b.footprint.iter())
            .map(|p| p.length())
            .fold(0.0, f64::max)
    }

    fn add_walls(&mut self) {
        let corners: Vec<Point> = self
            .placed
            .iter()
            .flat_map(|b| b.footprint.iter().copied())
            .collect();
        let hull = geometry::convex_hull(&corners);
        if hull.len() < 3 {
            return;
        }
        let c = geometry::centroid(&hull);
        let ring: Vec<Point> = hull
            .iter()
            .map(|v| v.plus(v.minus(c).normalised().scale(6.0)))
            .collect();
        let n = ring.len();
        let mut segments: Vec<Vec<Point>> = Vec::new();
        let mut run: Vec<Point> = Vec::new();
        let mut gates = Vec::new();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let mid = a.plus(b).scale(0.5);
            if self.in_water(mid) {
                if run.len() >= 2 {
                    segments.push(std::mem::take(&mut run));
                }
                run.clear();
                continue;
            }
            if run.is_empty() {
                run.push(a);
            }
            run.push(b);
            for street in self.streets.iter().filter(|s| s.kind == StreetKind::Road) {
                for pair in street.path.windows(2) {
                    if let Some(x) = geometry::segment_intersection(a, b, pair[0], pair[1]) {
                        gates.push(x);
                    }
                }
            }
        }
        if run.len() >= 2 {
            segments.push(run);
        }
        let towers = ring
            .iter()
            .copied()
            .filter(|p| !self.in_water(*p))
            .collect();
        if !segments.is_empty() {
            self.walls.push(Wall {
                id: "wall-1".into(),
                segments,
                gates,
                towers,
            });
        }
    }

    fn add_bridges(&mut self) {
        for w in 0..self.water.len() {
            if self.water[w].kind != WaterKind::River {
                continue;
            }
            let line = self.water[w].centreline.clone();
            let mut bridges = Vec::new();
            for street in &self.streets {
                for s in street.path.windows(2) {
                    for l in line.windows(2) {
                        if let Some(x) = geometry::segment_intersection(s[0], s[1], l[0], l[1]) {
                            bridges.push(x);
                        }
                    }
                }
            }
            self.water[w].bridges = bridges;
        }
    }

    fn add_fields(&mut self) {
        let inner = self.built_radius() + 12.0;
        let wanted = self.rng.int(4, 10) as usize;
        let roads: Vec<usize> = (0..self.streets.len())
            .filter(|&i| self.streets[i].kind == StreetKind::Road)
            .collect();
        let mut attempts = 0;
        while self.fields.len() < wanted && attempts < wanted * 12 && !roads.is_empty() {
            attempts += 1;
            let pick = roads[self.rng.int(0, roads.len() as u32 - 1) as usize];
            let path = self.streets[pick].path.clone();
            let half = self.streets[pick].width_m / 2.0;
            let k = self.rng.int(0, path.len() as u32 - 2) as usize;
            let (a, b) = (path[k], path[k + 1]);
            let u = b.minus(a).normalised();
            let r = self.p.radius_m;
            let along = (r * self.rng.range(0.22, 0.4)).clamp(12.0, 60.0);
            let deep = (r * self.rng.range(0.18, 0.35)).clamp(10.0, 50.0);
            let side = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            let centre = a
                .plus(b.minus(a).scale(self.rng.unit()))
                .plus(u.perpendicular().scale(side * (half + 2.0 + deep / 2.0)));
            let plot = geometry::oriented_rect(centre, u, along, deep);
            let ok = centre.length() > inner
                && plot
                    .iter()
                    .all(|p| self.bounds.contains(*p) && !self.in_water(*p))
                && self.clear_of_streets(&plot)
                && self
                    .fields
                    .iter()
                    .all(|f| !geometry::convex_overlap(&plot, &f.area))
                && self
                    .placed
                    .iter()
                    .all(|b| !geometry::convex_overlap(&plot, &b.clearance));
            if ok {
                self.fields.push(AreaFeature {
                    id: format!("field-{}", self.fields.len() + 1),
                    area: plot,
                });
            }
        }
    }

    fn add_trees(&mut self) {
        let wanted = (self.p.trees * 60.0).round() as usize;
        let inner = self.built_radius() + 6.0;
        let b = self.bounds;
        let mut attempts = 0;
        while self.trees.len() < wanted && attempts < wanted * 8 {
            attempts += 1;
            let centre = Point(
                self.rng.range(b.min_x, b.max_x),
                self.rng.range(b.min_y, b.max_y),
            );
            let radius_m = self.rng.range(4.0, 12.0);
            let clear_of_streets = self
                .segments
                .iter()
                .all(|s| geometry::segment_distance(centre, s.a, s.b) > s.half + radius_m);
            let ok = centre.length() > inner + radius_m
                && !self.in_water(centre)
                && clear_of_streets
                && self
                    .fields
                    .iter()
                    .all(|f| !geometry::contains(&f.area, centre));
            if ok {
                self.trees.push(TreeCluster { centre, radius_m });
            }
        }
    }

    // ----- output -----------------------------------------------------------

    fn finish(self) -> MapGeometry {
        let key = map_key(self.req.map_id.as_str()).to_owned();
        let street_ids: Vec<String> = self.streets.iter().map(|s| s.id.clone()).collect();
        let buildings = self
            .placed
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                let mut footprint = b.footprint;
                geometry::round_points(&mut footprint);
                let mut centroid = geometry::centroid(&footprint);
                geometry::round_point(&mut centroid);
                let area_m2 = (geometry::area(&footprint) * 100.0).round() / 100.0;
                MapBuilding {
                    id: BuildingId::new(format!("bld-{key}-r{}-{:04}", self.req.revision, i + 1)),
                    map_id: self.req.map_id.clone(),
                    settlement_id: self.req.settlement_id.clone(),
                    footprint,
                    centroid,
                    area_m2,
                    size: b.size,
                    street: b.street.and_then(|i| street_ids.get(i).cloned()),
                }
            })
            .collect();
        let mut streets = self.streets;
        for s in &mut streets {
            geometry::round_points(&mut s.path);
        }
        let mut walls = self.walls;
        for w in &mut walls {
            w.segments
                .iter_mut()
                .for_each(|s| geometry::round_points(s));
            geometry::round_points(&mut w.gates);
            geometry::round_points(&mut w.towers);
        }
        let mut water = self.water;
        for w in &mut water {
            geometry::round_points(&mut w.area);
            geometry::round_points(&mut w.centreline);
            geometry::round_points(&mut w.bridges);
        }
        let mut fields = self.fields;
        for f in &mut fields {
            geometry::round_points(&mut f.area);
        }
        let mut squares: Vec<AreaFeature> = self
            .square
            .into_iter()
            .map(|area| AreaFeature {
                id: "square-1".into(),
                area,
            })
            .collect();
        for s in &mut squares {
            geometry::round_points(&mut s.area);
        }
        let mut trees = self.trees;
        for t in &mut trees {
            geometry::round_point(&mut t.centre);
            t.radius_m = (t.radius_m * 100.0).round() / 100.0;
        }
        MapGeometry {
            bounds: self.bounds,
            buildings,
            streets,
            walls,
            water,
            terrain: Terrain {
                ground: self.p.ground,
                trees,
                fields,
                squares,
            },
        }
    }
}
