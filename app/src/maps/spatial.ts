// Hit testing by geometry. Buildings are found from their stored footprint
// polygons through a uniform grid, never from the drawn artwork, so a
// click or hover costs a handful of point-in-polygon tests however large the
// map. Building footprints never overlap, so at most one building contains a
// point.
import type { MapBuilding, Point } from "./types";

/** Grid cell size in metres: a little larger than the biggest building. */
const CELL_M = 24;

export interface SpatialIndex {
  /** The building at map point `p`, if any. */
  hit(p: Point): MapBuilding | null;
  /** How many buildings were tested by the last `hit` (for tests). */
  lastTests(): number;
}

/** Even-odd point-in-polygon test (as the core's `geometry::contains`). */
export function contains(ring: Point[], [px, py]: Point): boolean {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [ax, ay] = ring[i];
    const [bx, by] = ring[j];
    if (ay > py !== by > py && px < ((bx - ax) * (py - ay)) / (by - ay) + ax) inside = !inside;
  }
  return inside;
}

export function buildIndex(buildings: MapBuilding[]): SpatialIndex {
  const cells = new Map<string, MapBuilding[]>();
  const key = (x: number, y: number) => `${x},${y}`;
  const cell = (v: number) => Math.floor(v / CELL_M);
  for (const b of buildings) {
    let x0 = Infinity,
      y0 = Infinity,
      x1 = -Infinity,
      y1 = -Infinity;
    for (const [x, y] of b.footprint) {
      x0 = Math.min(x0, x);
      y0 = Math.min(y0, y);
      x1 = Math.max(x1, x);
      y1 = Math.max(y1, y);
    }
    for (let cx = cell(x0); cx <= cell(x1); cx++) {
      for (let cy = cell(y0); cy <= cell(y1); cy++) {
        const k = key(cx, cy);
        const list = cells.get(k);
        if (list) list.push(b);
        else cells.set(k, [b]);
      }
    }
  }
  let tests = 0;
  return {
    hit(p) {
      tests = 0;
      for (const b of cells.get(key(cell(p[0]), cell(p[1]))) ?? []) {
        tests++;
        if (contains(b.footprint, p)) return b;
      }
      return null;
    },
    lastTests: () => tests,
  };
}

const indexes = new WeakMap<MapBuilding[], SpatialIndex>();

/** The index of a map's buildings, built once per loaded map. */
export function indexFor(buildings: MapBuilding[]): SpatialIndex {
  let index = indexes.get(buildings);
  if (!index) {
    index = buildIndex(buildings);
    indexes.set(buildings, index);
  }
  return index;
}
