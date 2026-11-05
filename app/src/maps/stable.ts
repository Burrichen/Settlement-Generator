// Keeps map geometry objects stable across responses from the core.
//
// Every directory change (placing a location, adding a point, renaming a
// district) answers with the whole maps view, so each response carries fresh
// copies of every map's geometry. The layer paths and the hit-testing grid
// are cached per geometry object, and the static layers are memoised on it,
// so fresh copies would rebuild and redraw every map after every click.
// Stored geometry is immutable: a map only changes when it is redrawn, and a
// redraw always moves its revision. So a map with the same identity keeps the
// geometry object already on screen.
import type { SettlementMap, SettlementMapsView } from "./types";

const identity = (m: SettlementMap) => `${m.id}:${m.revision}:${m.seed}:${m.engine.version}`;

export function withStableGeometry(
  prev: SettlementMapsView | null,
  next: SettlementMapsView,
): SettlementMapsView {
  const before = prev?.maps?.maps;
  const after = next.maps?.maps;
  if (!before || !after || !next.maps) return next;
  const known = new Map(before.map((m) => [identity(m), m]));
  let reused = false;
  const maps = after.map((m) => {
    const old = known.get(identity(m));
    if (!old) return m;
    reused = true;
    return { ...m, geometry: old.geometry };
  });
  return reused ? { ...next, maps: { ...next.maps, maps } } : next;
}
