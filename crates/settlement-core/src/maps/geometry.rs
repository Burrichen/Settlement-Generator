//! Planar geometry for maps.
//!
//! **Frame (geometry version 1):** local planar coordinates in metres, origin
//! at the settlement's (or district's) centre, x east, y south (the SVG
//! convention). Bearings are degrees clockwise from north. Coordinates are
//! rounded to the centimetre when a map is finished, so saved geometry is
//! compact and compares exactly.

use serde::{Deserialize, Serialize};

/// A point `[x, y]` in metres. Serialised as a two-element array.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point(pub f64, pub f64);

impl Point {
    pub fn x(self) -> f64 {
        self.0
    }

    pub fn y(self) -> f64 {
        self.1
    }

    pub fn plus(self, o: Point) -> Point {
        Point(self.0 + o.0, self.1 + o.1)
    }

    pub fn minus(self, o: Point) -> Point {
        Point(self.0 - o.0, self.1 - o.1)
    }

    pub fn scale(self, k: f64) -> Point {
        Point(self.0 * k, self.1 * k)
    }

    pub fn dot(self, o: Point) -> f64 {
        self.0 * o.0 + self.1 * o.1
    }

    pub fn cross(self, o: Point) -> f64 {
        self.0 * o.1 - self.1 * o.0
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn distance(self, o: Point) -> f64 {
        self.minus(o).length()
    }

    /// Unit vector, or zero for the zero vector.
    pub fn normalised(self) -> Point {
        let l = self.length();
        if l == 0.0 { self } else { self.scale(1.0 / l) }
    }

    /// Rotated 90° clockwise on screen (y down).
    pub fn perpendicular(self) -> Point {
        Point(-self.1, self.0)
    }

    /// Unit vector for a bearing in radians, clockwise from north.
    pub fn from_bearing(bearing: f64) -> Point {
        Point(bearing.sin(), -bearing.cos())
    }

    fn rounded(self) -> Point {
        Point(round_cm(self.0), round_cm(self.1))
    }
}

fn round_cm(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    // Avoid "-0.0" in saved geometry.
    if r == 0.0 { 0.0 } else { r }
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    /// A square of half-width `half` centred on the origin.
    pub fn square(half: f64) -> Self {
        Bounds {
            min_x: -half,
            min_y: -half,
            max_x: half,
            max_y: half,
        }
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    pub fn contains(&self, p: Point) -> bool {
        p.0 >= self.min_x && p.0 <= self.max_x && p.1 >= self.min_y && p.1 <= self.max_y
    }

    pub fn corners(&self) -> Vec<Point> {
        vec![
            Point(self.min_x, self.min_y),
            Point(self.max_x, self.min_y),
            Point(self.max_x, self.max_y),
            Point(self.min_x, self.max_y),
        ]
    }
}

/// Signed area (positive when clockwise on screen, y down).
pub fn signed_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    (0..n)
        .map(|i| ring[i].cross(ring[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

pub fn area(ring: &[Point]) -> f64 {
    signed_area(ring).abs()
}

/// Area-weighted centroid; the vertex average for a degenerate ring.
pub fn centroid(ring: &[Point]) -> Point {
    let a = signed_area(ring);
    let n = ring.len();
    if a.abs() < 1e-9 {
        let sum = ring.iter().fold(Point(0.0, 0.0), |s, p| s.plus(*p));
        return sum.scale(1.0 / n.max(1) as f64);
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        let c = p.cross(q);
        cx += (p.0 + q.0) * c;
        cy += (p.1 + q.1) * c;
    }
    Point(cx / (6.0 * a), cy / (6.0 * a))
}

/// Even-odd point-in-polygon test. Used for hit-testing buildings.
pub fn contains(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance from `p` to the segment `a`–`b`.
pub fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let ab = b.minus(a);
    let len2 = ab.dot(ab);
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (p.minus(a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a.plus(ab.scale(t)))
}

/// Intersection point of segments `a`–`b` and `c`–`d`, if they cross.
pub fn segment_intersection(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let r = b.minus(a);
    let s = d.minus(c);
    let denom = r.cross(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = c.minus(a).cross(s) / denom;
    let u = c.minus(a).cross(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a.plus(r.scale(t)))
}

/// A rectangle of `width` × `depth` centred on `centre`, its width running
/// along the unit vector `along`.
pub fn oriented_rect(centre: Point, along: Point, width: f64, depth: f64) -> Vec<Point> {
    let u = along.scale(width / 2.0);
    let v = along.perpendicular().scale(depth / 2.0);
    vec![
        centre.minus(u).minus(v),
        centre.plus(u).minus(v),
        centre.plus(u).plus(v),
        centre.minus(u).plus(v),
    ]
}

/// Whether two convex polygons overlap (separating axis theorem).
pub fn convex_overlap(a: &[Point], b: &[Point]) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let axis = poly[(i + 1) % n].minus(poly[i]).perpendicular();
            let project = |ps: &[Point]| {
                ps.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| {
                    let d = p.dot(axis);
                    (lo.min(d), hi.max(d))
                })
            };
            let (a_lo, a_hi) = project(a);
            let (b_lo, b_hi) = project(b);
            if a_hi <= b_lo || b_hi <= a_lo {
                return false;
            }
        }
    }
    true
}

/// Convex hull (Andrew's monotone chain), clockwise on screen.
pub fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut pts: Vec<Point> = points.to_vec();
    pts.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let turn = |o: Point, a: Point, b: Point| a.minus(o).cross(b.minus(o));
    let mut lower: Vec<Point> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && turn(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && turn(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Clips a polygon (convex or not) to `bounds` (Sutherland–Hodgman).
pub fn clip_to_bounds(ring: &[Point], bounds: &Bounds) -> Vec<Point> {
    type Inside = fn(Point, &Bounds) -> bool;
    type Cut = fn(Point, Point, &Bounds) -> Point;
    let edges: [(Inside, Cut); 4] = [
        (
            |p, b| p.0 >= b.min_x,
            |a, c, b| {
                let t = (b.min_x - a.0) / (c.0 - a.0);
                Point(b.min_x, a.1 + (c.1 - a.1) * t)
            },
        ),
        (
            |p, b| p.0 <= b.max_x,
            |a, c, b| {
                let t = (b.max_x - a.0) / (c.0 - a.0);
                Point(b.max_x, a.1 + (c.1 - a.1) * t)
            },
        ),
        (
            |p, b| p.1 >= b.min_y,
            |a, c, b| {
                let t = (b.min_y - a.1) / (c.1 - a.1);
                Point(a.0 + (c.0 - a.0) * t, b.min_y)
            },
        ),
        (
            |p, b| p.1 <= b.max_y,
            |a, c, b| {
                let t = (b.max_y - a.1) / (c.1 - a.1);
                Point(a.0 + (c.0 - a.0) * t, b.max_y)
            },
        ),
    ];
    let mut out = ring.to_vec();
    for (inside, cut) in edges {
        let input = std::mem::take(&mut out);
        let n = input.len();
        for i in 0..n {
            let cur = input[i];
            let prev = input[(i + n - 1) % n];
            match (inside(cur, bounds), inside(prev, bounds)) {
                (true, true) => out.push(cur),
                (true, false) => {
                    out.push(cut(prev, cur, bounds));
                    out.push(cur);
                }
                (false, true) => out.push(cut(prev, cur, bounds)),
                (false, false) => {}
            }
        }
    }
    out
}

/// The parts of a polyline inside `bounds`, each ending exactly on the edge
/// where the line leaves.
pub fn clip_polyline(path: &[Point], bounds: &Bounds) -> Vec<Vec<Point>> {
    let corners = bounds.corners();
    let crossing = |a: Point, b: Point| {
        (0..4).find_map(|i| segment_intersection(a, b, corners[i], corners[(i + 1) % 4]))
    };
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut run: Vec<Point> = Vec::new();
    for (i, &p) in path.iter().enumerate() {
        let inside = bounds.contains(p);
        if i > 0 && inside != bounds.contains(path[i - 1]) {
            run.extend(crossing(path[i - 1], p));
        }
        if inside {
            run.push(p);
        } else if !run.is_empty() {
            if run.len() >= 2 {
                runs.push(std::mem::take(&mut run));
            }
            run.clear();
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

/// Rounds every point to the centimetre.
pub fn round_points(points: &mut [Point]) {
    for p in points {
        *p = p.rounded();
    }
}

pub fn round_point(p: &mut Point) {
    *p = p.rounded();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_square() -> Vec<Point> {
        vec![
            Point(0.0, 0.0),
            Point(1.0, 0.0),
            Point(1.0, 1.0),
            Point(0.0, 1.0),
        ]
    }

    #[test]
    fn area_centroid_and_containment() {
        let sq = unit_square();
        assert!((area(&sq) - 1.0).abs() < 1e-12);
        assert_eq!(centroid(&sq), Point(0.5, 0.5));
        assert!(contains(&sq, Point(0.5, 0.5)));
        assert!(!contains(&sq, Point(1.5, 0.5)));
    }

    #[test]
    fn overlap_and_intersection() {
        let a = unit_square();
        let b: Vec<Point> = a.iter().map(|p| p.plus(Point(0.5, 0.5))).collect();
        let c: Vec<Point> = a.iter().map(|p| p.plus(Point(2.0, 0.0))).collect();
        assert!(convex_overlap(&a, &b));
        assert!(!convex_overlap(&a, &c));
        let x = segment_intersection(
            Point(0.0, 0.0),
            Point(2.0, 2.0),
            Point(0.0, 2.0),
            Point(2.0, 0.0),
        );
        assert_eq!(x, Some(Point(1.0, 1.0)));
    }

    #[test]
    fn hull_and_clip() {
        let mut pts = unit_square();
        pts.push(Point(0.5, 0.5));
        assert_eq!(convex_hull(&pts).len(), 4);
        let big = Bounds::square(2.0).corners();
        let clipped = clip_to_bounds(&big, &Bounds::square(1.0));
        assert!((area(&clipped) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn bearings_point_the_right_way() {
        let north = Point::from_bearing(0.0);
        assert!((north.1 + 1.0).abs() < 1e-12, "north is up (negative y)");
        let east = Point::from_bearing(std::f64::consts::FRAC_PI_2);
        assert!((east.0 - 1.0).abs() < 1e-12);
    }
}
