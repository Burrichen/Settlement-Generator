// Viewport maths, geometric hit testing and layer caching.
import { describe, expect, it } from "vitest";
import { layerBuildCount, layersFor } from "./paths";
import { buildIndex, contains } from "./spatial";
import { grid, makeMap } from "./testMaps";
import type { Point } from "./types";
import {
  MAX_ZOOM_FACTOR,
  MIN_ZOOM_FACTOR,
  fitBounds,
  pan,
  toMap,
  toScreen,
  zoomAt,
  zoomLimits,
} from "./viewport";

const size = { width: 800, height: 600 };
const bounds = { min_x: -100, min_y: -50, max_x: 100, max_y: 50 };

describe("viewport", () => {
  it("fits bounds centred within the padding", () => {
    const v = fitBounds(bounds, size, 0);
    expect(v.k).toBe(4); // 800 px / 200 m
    expect(toScreen(v, [0, 0])).toEqual([400, 300]);
    expect(toScreen(v, [-100, -50])).toEqual([0, 100]);
  });

  it("zooms about the pointer: the map point under it stays put", () => {
    const v = fitBounds(bounds, size);
    const limits = zoomLimits(bounds, size);
    const under = toMap(v, 250, 120);
    const z = zoomAt(v, 2, 250, 120, limits);
    expect(z.k).toBeCloseTo(v.k * 2);
    const [x, y] = toScreen(z, under);
    expect(x).toBeCloseTo(250);
    expect(y).toBeCloseTo(120);
  });

  it("clamps zoom to the limits", () => {
    const v = fitBounds(bounds, size);
    const limits = zoomLimits(bounds, size);
    expect(zoomAt(v, 1000, 0, 0, limits).k).toBeCloseTo(v.k * MAX_ZOOM_FACTOR);
    expect(zoomAt(v, 0.0001, 0, 0, limits).k).toBeCloseTo(v.k * MIN_ZOOM_FACTOR);
  });

  it("pans by pixels and converts both ways", () => {
    const v = pan(fitBounds(bounds, size), 30, -10);
    const p: Point = [12.5, -7];
    const [sx, sy] = toScreen(v, p);
    const back = toMap(v, sx, sy);
    expect(back[0]).toBeCloseTo(p[0]);
    expect(back[1]).toBeCloseTo(p[1]);
  });
});

describe("hit testing", () => {
  const buildings = grid("map-settlement", 600);
  const index = buildIndex(buildings);

  it("finds the building under a point, and nothing between buildings", () => {
    const b = buildings[123];
    expect(index.hit(b.centroid)?.id).toBe(b.id);
    expect(index.hit([b.centroid[0] + 3.9, b.centroid[1] - 3.9])?.id).toBe(b.id);
    // The 4 m gap between neighbours on the 12 m grid.
    expect(index.hit([b.centroid[0] + 6, b.centroid[1]])).toBeNull();
    expect(index.hit([10_000, 10_000])).toBeNull();
  });

  it("agrees with testing every building, by geometry", () => {
    let seed = 1;
    const random = () => ((seed = (seed * 48271) % 2147483647) / 2147483647) * 300 - 150;
    for (let i = 0; i < 2000; i++) {
      const p: Point = [random(), random()];
      const brute = buildings.find((b) => contains(b.footprint, p))?.id ?? null;
      expect(index.hit(p)?.id ?? null).toBe(brute);
    }
  });

  it("tests only a few buildings per query, and is fast", () => {
    const t0 = performance.now();
    let worst = 0;
    for (let i = 0; i < 10_000; i++) {
      const b = buildings[i % buildings.length];
      index.hit(b.centroid);
      worst = Math.max(worst, index.lastTests());
    }
    const elapsed = performance.now() - t0;
    expect(worst).toBeLessThanOrEqual(6);
    expect(elapsed).toBeLessThan(250);
  });
});

describe("layers", () => {
  it("merge every building into one path and are built once per map", () => {
    const map = makeMap("map-settlement", "Testford", grid("map-settlement", 600));
    const before = layerBuildCount();
    const layers = layersFor(map.geometry);
    expect(layers.buildings.match(/M/g)).toHaveLength(600);
    expect(layersFor(map.geometry)).toBe(layers);
    expect(layerBuildCount()).toBe(before + 1);
  });
});
