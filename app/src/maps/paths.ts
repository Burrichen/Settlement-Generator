// SVG path data for a map's static layers. Each layer is ONE path, not one
// element per building: profiling (see docs/MAPS.md §11) showed merged
// layers keep pan and zoom within a frame at the core's 600-building limit
// while keeping the DOM to a few nodes. Built once per map and cached.
import type { MapGeometry, Point, StreetKind } from "./types";

export interface MapLayers {
  buildings: string;
  streets: Record<StreetKind, string>;
  /** Width in metres of each kind of street on this map. */
  streetWidths: Record<StreetKind, number>;
  water: string;
  walls: string;
  towers: Point[];
  gates: Point[];
  bridges: Point[];
  fields: string;
  squares: string;
  trees: { centre: Point; radius: number }[];
  /** Outline of the built area (convex hull), for the district layer. */
  outline: string;
}

const n = (v: number) => Math.round(v * 100) / 100;

export function ringPath(ring: Point[]): string {
  if (ring.length === 0) return "";
  return `M${ring.map(([x, y]) => `${n(x)} ${n(y)}`).join("L")}Z`;
}

export function linePath(line: Point[]): string {
  if (line.length === 0) return "";
  return `M${line.map(([x, y]) => `${n(x)} ${n(y)}`).join("L")}`;
}

/** Convex hull (monotone chain). */
export function hull(points: Point[]): Point[] {
  const pts = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (pts.length < 3) return pts;
  const cross = (o: Point, a: Point, b: Point) =>
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: Point[] = [];
  for (const p of pts) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 0)
      lower.pop();
    lower.push(p);
  }
  const upper: Point[] = [];
  for (let i = pts.length - 1; i >= 0; i--) {
    const p = pts[i];
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 0)
      upper.pop();
    upper.push(p);
  }
  return lower.slice(0, -1).concat(upper.slice(0, -1));
}

let builds = 0;
const cache = new WeakMap<MapGeometry, MapLayers>();

/** The layers of `g`, computed on first use. */
export function layersFor(g: MapGeometry): MapLayers {
  const cached = cache.get(g);
  if (cached) return cached;
  builds++;
  const streets: Record<StreetKind, string> = { road: "", street: "", lane: "" };
  const streetWidths: Record<StreetKind, number> = { road: 0, street: 0, lane: 0 };
  for (const s of g.streets) {
    streets[s.kind] += linePath(s.path);
    streetWidths[s.kind] = Math.max(streetWidths[s.kind], s.width_m);
  }
  const layers: MapLayers = {
    buildings: g.buildings.map((b) => ringPath(b.footprint)).join(""),
    streets,
    streetWidths,
    water: (g.water ?? []).map((w) => ringPath(w.area)).join(""),
    walls: (g.walls ?? []).flatMap((w) => w.segments.map(linePath)).join(""),
    towers: (g.walls ?? []).flatMap((w) => w.towers),
    gates: (g.walls ?? []).flatMap((w) => w.gates),
    bridges: (g.water ?? []).flatMap((w) => w.bridges ?? []),
    fields: (g.terrain.fields ?? []).map((f) => ringPath(f.area)).join(""),
    squares: (g.terrain.squares ?? []).map((f) => ringPath(f.area)).join(""),
    trees: (g.terrain.trees ?? []).map((t) => ({ centre: t.centre, radius: t.radius_m })),
    outline: ringPath(hull(g.buildings.flatMap((b) => b.footprint))),
  };
  cache.set(g, layers);
  return layers;
}

/** How many times layers have been built (tests check it stays at one). */
export function layerBuildCount(): number {
  return builds;
}
