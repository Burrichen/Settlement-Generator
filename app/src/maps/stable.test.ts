import { describe, expect, it } from "vitest";
import { layerBuildCount, layersFor } from "./paths";
import { withStableGeometry } from "./stable";
import { districtMap, plannedDistricts, view } from "./testMaps";

/** The same maps as a fresh response from the core: equal, but new objects. */
const fresh = <T>(v: T): T => JSON.parse(JSON.stringify(v));

describe("withStableGeometry", () => {
  it("keeps the geometry on screen when a response carries the same maps", () => {
    const before = view([districtMap(1), districtMap(2)], plannedDistricts([true, true]));
    const after = withStableGeometry(before, fresh(before));
    expect(after.maps!.maps[0].geometry).toBe(before.maps!.maps[0].geometry);
    expect(after.maps!.maps[1].geometry).toBe(before.maps!.maps[1].geometry);
    // The cached layer paths are reused, not rebuilt.
    layersFor(before.maps!.maps[0].geometry);
    const builds = layerBuildCount();
    layersFor(after.maps!.maps[0].geometry);
    expect(layerBuildCount()).toBe(builds);
  });

  it("takes the new geometry of a redrawn map and keeps the others", () => {
    const before = view([districtMap(1), districtMap(2)], plannedDistricts([true, true]));
    const next = fresh(before);
    next.maps!.maps[1].revision = 2;
    const after = withStableGeometry(before, next);
    expect(after.maps!.maps[0].geometry).toBe(before.maps!.maps[0].geometry);
    expect(after.maps!.maps[1].geometry).toBe(next.maps!.maps[1].geometry);
    expect(after.maps!.maps[1].geometry).not.toBe(before.maps!.maps[1].geometry);
  });

  it("passes everything else through from the new view", () => {
    const before = view([districtMap(1)], plannedDistricts([true]));
    const next = fresh(before);
    next.maps!.maps[0].label = "Harbour Ward";
    next.status = "stale";
    const after = withStableGeometry(before, next);
    expect(after.status).toBe("stale");
    expect(after.maps!.maps[0].label).toBe("Harbour Ward");
    expect(withStableGeometry(null, next)).toBe(next);
  });
});
