// Viewport maths for the map canvas: map metres → screen pixels as
// `screen = map * k + t`. Pure functions, so they are easy to test.
import type { Bounds, Point } from "./types";

export interface Viewport {
  /** Pixels per metre. */
  k: number;
  tx: number;
  ty: number;
}

export interface Size {
  width: number;
  height: number;
}

/** Zoom limits, relative to the zoom that fits the whole map. */
export const MIN_ZOOM_FACTOR = 0.5;
export const MAX_ZOOM_FACTOR = 24;
/** Each wheel notch or zoom button press. */
export const ZOOM_STEP = 1.25;
/** Arrow keys pan by this many pixels. */
export const PAN_STEP = 60;

/** The view showing `bounds` whole, centred, with `padding` pixels spare. */
export function fitBounds(bounds: Bounds, size: Size, padding = 16): Viewport {
  const w = Math.max(bounds.max_x - bounds.min_x, 1);
  const h = Math.max(bounds.max_y - bounds.min_y, 1);
  const k = Math.max(
    Math.min((size.width - 2 * padding) / w, (size.height - 2 * padding) / h),
    1e-6,
  );
  return {
    k,
    tx: size.width / 2 - ((bounds.min_x + bounds.max_x) / 2) * k,
    ty: size.height / 2 - ((bounds.min_y + bounds.max_y) / 2) * k,
  };
}

/** Zoom by `factor` keeping the map point under (px, py) where it is. */
export function zoomAt(
  v: Viewport,
  factor: number,
  px: number,
  py: number,
  limits: { min: number; max: number },
): Viewport {
  const k = Math.min(Math.max(v.k * factor, limits.min), limits.max);
  const f = k / v.k;
  return { k, tx: px - (px - v.tx) * f, ty: py - (py - v.ty) * f };
}

export function pan(v: Viewport, dx: number, dy: number): Viewport {
  return { k: v.k, tx: v.tx + dx, ty: v.ty + dy };
}

/** The view with map point `p` at the centre of the canvas. */
export function centreOn(v: Viewport, p: Point, size: Size): Viewport {
  return { k: v.k, tx: size.width / 2 - p[0] * v.k, ty: size.height / 2 - p[1] * v.k };
}

/** Screen pixel (relative to the canvas) → map metres. */
export function toMap(v: Viewport, px: number, py: number): Point {
  return [(px - v.tx) / v.k, (py - v.ty) / v.k];
}

export function toScreen(v: Viewport, p: Point): Point {
  return [p[0] * v.k + v.tx, p[1] * v.k + v.ty];
}

/** Zoom limits for a map shown at `size`. */
export function zoomLimits(bounds: Bounds, size: Size) {
  const fit = fitBounds(bounds, size).k;
  return { min: fit * MIN_ZOOM_FACTOR, max: fit * MAX_ZOOM_FACTOR };
}

/** Bounds of a set of rings, grown by `margin` metres. */
export function boundsOf(rings: Point[][], margin = 0): Bounds | null {
  let b: Bounds | null = null;
  for (const ring of rings) {
    for (const [x, y] of ring) {
      if (!b) b = { min_x: x, min_y: y, max_x: x, max_y: y };
      else {
        b.min_x = Math.min(b.min_x, x);
        b.min_y = Math.min(b.min_y, y);
        b.max_x = Math.max(b.max_x, x);
        b.max_y = Math.max(b.max_y, y);
      }
    }
  }
  if (!b) return null;
  return {
    min_x: b.min_x - margin,
    min_y: b.min_y - margin,
    max_x: b.max_x + margin,
    max_y: b.max_y + margin,
  };
}
