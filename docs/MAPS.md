# Interactive Settlement Maps

> **Status:** in development on `feature/interactive-settlement-maps`.
> Phases 1 (architecture), 2 (engine integration), 3 (Map Explorer GUI),
> 4 (lazy building details and the editable Location Inspector), 5
> (Notable Locations on buildings, numbered markers and the legend) and 6
> (district maps: names, points of interest, saved views, the
> settlement-wide directory) and 7 (persistence, exports, imports,
> backward compatibility and the terminal interface) are done. Maps are generated, validated, stored and read back through the
> shared core, the GUI shows them in a Map tab (§11), each building's
> details are generated the first time it is opened (§12), and the
> settlement's Notable Locations can be placed on buildings (§13). The terminal
> interface has Maps and Building screens over the same service (§15). See [§10 Checklist](#10-implementation-checklist).

For a plain-words user guide, see [MAPS_GUIDE.md](MAPS_GUIDE.md).

Source: `crates/settlement-core/src/maps/`, `src/db/maps.rs`,
`src/db/migrations/0002_settlement_maps.sql`,
`src/db/migrations/0003_map_building_details.sql`,
`src/db/migrations/0004_notable_location_directory.sql`,
`src/db/migrations/0005_districts_pois_views.sql`,
`data/map-defaults.json`, `content/17-buildings/`, `app/src/maps/types.ts`.
Tests: `tests/maps.rs`, `tests/building_details.rs`, `tests/map_locations.rs`,
`tests/map_districts.rs`,
`src/db/upgrade_tests.rs`, `app/src/maps/*.test.tsx`.

## 1. Branch and base

- The branch started at the **v0.1.1** tag (`0994f79`), as first requested.
- Before any map work was committed, `main` moved on to **v0.1.5**
  (`1ef3dc3`), which added Notable Locations and per-district counts. The
  user chose to rebase the still-empty branch onto the `v0.1.5` tag, so maps
  can attach to the real districts and Notable Locations.
- Every settlement rule, probability and table is unchanged. Maps only read
  saved settlements.

## 2. How maps fit the existing architecture

The existing rules (see [ARCHITECTURE.md](ARCHITECTURE.md)) still hold:

- **All logic is in `settlement-core`.** Planning, generation, validation and
  storage of maps are core functions. `AppService::maps_get`,
  `maps_generate`, `maps_generate_with` and `maps_delete` are the only entry
  points. Tauri exposes them as `maps_get`, `maps_generate` and
  `maps_delete`, and the terminal interface will call the same methods.
- **Snapshots are never re-resolved.** Map inputs are copied from the saved
  snapshot (stage results, tags, districts, Notable Locations), never from
  current content. Generating maps never changes the settlement.
- **IDs are separate from names.** Map, building and token IDs are stable
  and never derived from display text.
- **Data, not code.** Which parameters follow from which tags is in
  `data/map-defaults.json`. It uses the content's own `Condition` format.
- **User data lives in the SQLite database**, migrated transactionally with
  a verified backup first.

What the codebase did **not** have at v0.1.5, so the map system does not
assume it:

- **Exports:** none existed at v0.1.5 (no file export of settlements). Map
  and settlement export were added in phase 7 (§15). Map export was new
  work.
- **Population:** no table records one. `MapInputs.population` exists for
  the future and is always absent. No parameter is derived from population.
- **Districts:** only City and Capital Notable Locations record them
  (`district-1` …, label "District 1" …), with no names, types or
  adjacency.

## 3. Engine investigation: settlemaker

Investigated 2026-10-11 against the published npm package (not the
repository head).

| Question             | Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Published versions   | `settlemaker` **3.0.0** and **3.0.1** only, both 2026-09-12; `latest` = 3.0.1. Earlier tags (2.x) were never published. There is no `@barrulus/settlemaker`.                                                                                                                                                                                                                                                                                                          |
| Unreleased work      | GitHub `master` has one more merge (PR #13, 2026-09-13: "shared appearance and engine choice", physical city planning, an oversized-temple fix). It is **not** in 3.0.1. Open issues #3–#6: small harbours, rivers through settlements, more parameters, names/ward labels/windrose. A single maintainer, 6 stars.                                                                                                                                                    |
| Stable API (3.0.1)   | `generateSettlement(burg, { seed, skin, svg, geojson, village })` is synchronous. It picks the **village planner** at population ≤ 1,000 and the **city planner** above that. `burg` needs `name`, `population`, `port`, `citadel`, `walls`, `plaza`, `temple`, `shanty` and `capital`. It returns `{ kind, model, svg, geojson, degradedFlags, originShift }`. Lower-level `generateFromBurg`, `generateVillage`, `buildScene` and tiling helpers are also exported. |
| Dependencies         | `polygon-clipping` (MIT) and `@types/geojson`. No install scripts.                                                                                                                                                                                                                                                                                                                                                                                                    |
| Browser / Tauri      | `dist/settlemaker.browser.js` (2.3 MB ESM) has no `fetch`, DOM or Node imports. `process` is only read behind `typeof process` guards. It would bundle with Vite. Without a `seed` it falls back to `Date.now()` / `Math.random()`, so a seed must always be passed.                                                                                                                                                                                                  |
| Geometry and IDs     | GeoJSON (schema 4) uses local planar coordinates, **not** WGS84. City units are estimated "settlement units"; village units are metres. City buildings are polygons with `building_id` (`b0`…) and `wardType`. Village buildings have `building_id`, `lot_id`, `occupancy` and more. POIs carry `building_id` (city) or not (village). IDs are "deterministic for identical inputs and a fixed generator version", **not stable across releases**.                    |
| SVG ↔ GeoJSON        | Matched by `data-building-id`. Measured: villages match 100%. Cities miss 2–3 buildings per map (for example `b182`, `b554`): park-ward buildings are drawn as greenery and have no SVG element. **So hit-testing must use GeoJSON polygons, not the SVG.**                                                                                                                                                                                                           |
| Determinism          | Same seed and input gave byte-identical SVG and GeoJSON (apart from `generated_at`) in every run. The RNG is a Park–Miller LCG (31-bit).                                                                                                                                                                                                                                                                                                                              |
| Performance (Node)   | Population 200: 70–150 ms; 800: 450–700 ms (village planner, the slowest); 3,000: 80–135 ms; 10,000: 120–200 ms; 40,000: 400–460 ms. SVG output was 1.2–6 MB per map.                                                                                                                                                                                                                                                                                                 |
| Limitations relevant | No district concept: a city is one map (wards are not districts). Two planners with different schemas. No names or labels. Rivers only in villages. Large SVGs. IDs churn between releases.                                                                                                                                                                                                                                                                           |

## 4. Licensing

**Findings**

- settlemaker is **GPL-3.0-only**. Its NOTICE says the generation core is a
  derivative of watabou's TownGeneratorOS (GPL-3.0) and lists the ported
  files.
- Its symbol artwork is separately licensed **CC-BY-4.0, with a rendered
  output exception**. Rendered maps carry no obligation; redistributing the
  symbol files requires attribution. Exception: the "Copperline" artwork is
  GPL-3.0-only.
- This application is distributed as a binary installer from a **public**
  GitHub repository that has **no licence file**. `Cargo.toml` declares
  `license = "UNLICENSED"`.

**What bundling settlemaker would mean** (an engineering reading, not legal
advice):

- Bundling it into the Vite build makes the shipped frontend a combined work
  with GPL code. Every distributed installer would then have to be offered
  under GPL-3.0, with the complete corresponding source of the combined work
  (in practice the whole app). It would also need the GPL text and notices
  in the installer and About/Credits screen.
- Loading it from a separate file, an iframe or a Web Worker inside the same
  app does not clearly change this: the parts still exchange intimate data
  structures and ship together.
- A **separate program** run at arm's length (as Azgaar's FMG does over a
  URL) can avoid combination, but it is a poor fit for an offline desktop
  app.
- Relicensing from barrulus alone would not be enough, because the core
  derives from watabou's GPL code.

**Decision for this phase:** settlemaker is **not** installed, copied or
bundled. Maps are drawn by an engine-neutral architecture
(`MapEngineAdapter`) with an original built-in engine (§7), which carries no
licence obligations.

**Unresolved decisions (for the owner):**

1. What licence the application itself is under. The public repository has
   none, so it is "all rights reserved" by default.
2. Whether to adopt settlemaker, which in practice means licensing the whole
   app GPL-3.0-only and publishing its source with each release.
3. If adopted: how a JavaScript engine would serve the terminal interface,
   which has no JavaScript runtime. Options are a bundled JS runtime
   sidecar, GUI-only generation (the TUI would show stored maps only), or a
   Rust port (also GPL).

Until these are settled, nothing GPL enters the repository.

## 5. Data model (`maps/model.rs`)

**Procedural geometry and editable metadata are kept strictly apart.**

| Type                                                     | Kind                     | Notes                                                                                                                                                                                                                            |
| -------------------------------------------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MapInputs`                                              | copied facts             | Settlement name, Size, Region, Environment, Connections, Origin, Purpose, Age, active tags, districts, and `population` (always absent). Read from snapshot stages by stage table ID (listed in `map-defaults.json` → `inputs`). |
| `MapParameters`, `MapParameterOverrides`, `MapOverrides` | drawing choices          | The defaults the tags select, then manual overrides: first for all maps, then per map.                                                                                                                                           |
| `MapGenerationRequest`                                   | engine input             | Settlement ID, map ID, scope, label, revision, derived seed, parameters, inputs, and the Notable Location IDs in this map's area.                                                                                                |
| `MapGenerationResult`                                    | engine output            | `EngineInfo` (ID, version, supported `MapFeature`s), geometry version, `MapGeometry` and `MapStats` (requested and placed, so a shortfall is reported, never hidden).                                                            |
| `SettlementMap` / `DistrictMap`                          | **geometry** (immutable) | One map: request header, engine and geometry. `DistrictMap` is a `SettlementMap` with a district scope.                                                                                                                          |
| `MapGeometry`                                            | **geometry**             | `bounds`, `buildings`, `streets`, `walls`, `water` and `terrain` (ground, trees, fields, squares).                                                                                                                               |
| `MapBuilding`                                            | **geometry**             | `id`, `map_id`, `settlement_id`, `footprint` (also the hit-test shape), `centroid`, `area_m2`, `size` and the street it faces. Has no details of its own.                                                                        |
| `SettlementMapSet`                                       | stored unit              | Every map of one settlement plus layout, seed, revision, overrides, inputs and their fingerprint, `generated_at`, `app_version` and the metadata.                                                                                |
| `MapMetadata`                                            | **editable**             | `placements` (`NotableLocationPlacement`) and `pois` (`PointOfInterest`). Building details are stored per building instead (§12).                                                                                                |
| `BuildingRecord` (`maps/details.rs`)                     | **editable**             | One building's details: `generated` values (each with its source) kept apart from the user's `edits`. No record means ungenerated. See §12.                                                                                      |
| `NotableLocation`                                        | settlement fact          | The existing `locations::NotableLocation` (`notable-location-NN`) from the snapshot. Maps refer to it by ID and never copy or alter it.                                                                                          |
| `MapMarker`, `MapLegendEntry`                            | derived                  | Computed by `maps::markers` from the location directory and tokens, never stored. Each carries the location's persistent marker number (§13).                                                                                    |
| `MapEngineAdapter`                                       | trait                    | `info()` and `generate(&request)`. Must be deterministic and must not generate details.                                                                                                                                          |

**Coordinates (geometry version 1):** local metres, origin at the map's
centre, x east, y south (SVG), bearings clockwise from north. All stored
coordinates are rounded to the centimetre and bounds to whole metres, so
saved geometry reads back exactly.

## 6. Identity, seeds and versions

| Item           | Rule                                                                                                                                                                                                                                                                                                              |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Map ID         | `map-settlement` (Trading Post, Village, Town) or `map-<district id>` (`map-district-2`). Unique within the settlement.                                                                                                                                                                                           |
| Building ID    | `bld-<map key>-r<revision>-<nnnn>`, e.g. `bld-district-2-r1-0042`. Unique within the settlement; `(settlement_id, building_id)` is unique everywhere. **Including the revision means a regenerated map never gives an old ID to a different building**, so stored details can never attach to the wrong building. |
| Set seed       | Random when maps are first generated, then saved. `SeedChoice::Keep` reuses it, `Random` draws a new one, `Fixed(n)` uses `n`. Capped at 2^53 − 1 so it survives JavaScript numbers.                                                                                                                              |
| Map seed       | `derive_seed(set seed, map ID)`: each district is independent, and adding or removing one never moves another.                                                                                                                                                                                                    |
| RNG            | Maps use their own SplitMix64 (`maps/rng.rs`), not `rand`'s `StdRng`, which does not promise the same output across `rand` releases. A dependency update cannot move a building.                                                                                                                                  |
| Engine version | `EngineInfo.version` (`builtin.medieval` 1.0.0). Same request and same engine version give identical geometry (tested). **Bump it whenever a change would draw different geometry.**                                                                                                                              |
| Geometry ver.  | `GEOMETRY_VERSION` = 1: the frame, units and feature shapes. Results with another version are refused.                                                                                                                                                                                                            |
| Map format     | `MAP_FORMAT_VERSION` = 1: the stored JSON documents (§8).                                                                                                                                                                                                                                                         |
| Staleness      | `inputs_fingerprint` (FNV-1a of `MapInputs`). If an edited settlement no longer matches, `MapStatus::Stale` is reported. Saved maps are kept as they are until the user regenerates.                                                                                                                              |

## 7. Generation

**Which maps**, from `map-defaults.json` → `sizes` (by Size tag):

- Trading Post, Village and Town: `single`, so one map of the whole
  settlement.
- City and Capital: `per_district`, so one independent map per district
  recorded by the Notable Locations stage, in district order, and **no
  whole-city map**.
- A City or Capital without recorded districts is refused with a message
  (`MapError::NoDistricts`), never given invented ones. So is a settlement
  without a Size (`NoSize`).
- Each map lists the Notable Locations in its area. The settlement-level
  guaranteed Tavern of a City or Capital (Bare Minimum Buildings) has no
  district, so it is not yet on any district map.

**Parameters**, in order:

1. `base`;
2. the Size's `set`;
3. each `rules` entry whose `when` holds, in file order (later rules win);
4. overrides for all maps;
5. overrides for this map.

They are then checked: 1–600 buildings, 20–600 m radius, at most 8 roads,
irregularity and tree cover 0–1. Summary of the defaults:

| Source              | Effect                                                                                                                                             |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Size                | Buildings per map (8 / 32 / 90 / 140 per district / 170 per district), radius, Town walls, squares (Town, City, Capital), village fields           |
| Connections         | Roads leaving the map: Isolated 1, Local Road 2, Regional Road 3, Major Route 4                                                                    |
| Environment, Region | Ground palette and tree cover. Coast and Coastlands give a coast; Wetlands give a lake; Subterranean gives a cavern with no water, trees or fields |
| Age, Origin         | Irregularity: New 0.25 … Ancient 0.75; Purpose-built 0.2; Organic Growth 0.7. Military Outpost walls a non-district map                            |
| Purpose             | Agriculture adds fields                                                                                                                            |

These are drawing choices, not settlement facts. Any of them can be
overridden per settlement or per map. `MapInputs` keeps the facts
themselves, so a later engine can use them differently.

**Built-in engine** (`maps/builtin.rs`, `builtin.medieval` 1.0.0): an
original street-and-lot planner written for this project, not derived from
watabou or settlemaker.

1. Water first: a coast, a meandering river, or a lake.
2. Roads leave along the recorded number of bearings, turning away from coast
   and lake. Fewer than two roads gives an internal high street.
3. A central square, if requested, is never more than 30% of the radius.
4. Growth outward from the centre in 10 m bands. Lots are offered along both
   sides of every street and accepted nearest first when they stay clear of
   water, streets, the square and other buildings.
5. Ring streets (with spokes between them) are added as growth reaches them,
   when the roads alone cannot hold the settlement or it has a square.
6. The frame is cropped to the built area plus countryside, and streets and
   water are re-clipped to it.
7. Walls are an offset hull around the built area, split where they meet
   water, with gates where roads cross and towers at the corners. Then river
   bridges, fields along roads outside the built area, and tree clusters.

Every result passes `engine::validate_result` before it can be stored. It
checks geometry version, non-empty bounds and buildings, unique IDs in this
map's namespace, parent IDs, real footprints inside the bounds, and honest
counts. An engine error or panic becomes a `MapError`. A Capital district of
170 buildings takes milliseconds; the largest allowed map (600 buildings,
600 m) takes well under a second in debug builds.

**Capability gaps (built-in engine 1.0.0):**

- Layouts are schematic: concentric ring streets with spokes, and
  rectangular lots only. There are no irregular blocks, courtyards, or
  L-shaped or merged buildings.
- Buildings have no function or type (only a small/medium/large footprint
  class). Nothing is assigned to Notable Locations yet.
- District maps are independent mini-plans. There is no shared city
  outline, no continuity at district edges, no city walls (walls on
  district maps are off by default), and no district character such as
  docks or a market quarter.
- Water: one feature per map. No rivers through gates, harbours, piers or
  docks. Coasts are straight-ish lines.
- No terrain relief, elevation, cliffs, or road and building response to
  hills or mountains. Ground is a palette key only.
- No names, labels, compass or scale bar in the geometry. These belong to
  rendering.
- Fields and trees are decoration, not land use.

## 8. Persistence and migration

Migration **0002_settlement_maps** adds two tables:

- `settlement_map_sets`: one row per settlement with maps. It holds
  `map_format`, `revision`, `generated_at`, `app_version`, `header` (layout,
  seed, overrides, inputs and fingerprint) and `metadata` (editable data, in
  its own column).
- `settlement_maps`: one row per map, with `position`, `map_format`,
  `engine`, `engine_version` and `document` (the `SettlementMap` with its
  geometry).

Rules:

- `settlement_maps` → `settlement_map_sets` → `settlements` cascade on
  delete. Deleting a settlement (never a favourite) removes its maps.
- A map set is generated and validated **entirely in memory**, then written
  in **one transaction**. A failed generation saves nothing and keeps the
  previous maps and revision. The settlement row is never written by map
  code.
- Reading a newer or damaged map document is an error for the maps only.
  The settlement still opens, and `maps_delete` still works.
- Opening a settlement reads the stored geometry. Nothing is regenerated
  implicitly, not even after an app or engine update.

**Migration strategy:**

- **Database schema:** numbered migrations as before. The upgrade tests now
  run a simulated "next" migration relative to the current latest. A test
  opens a real schema-1 (v0.1.5) database with saved settlements, checks
  the verified `schema1-to-schema2` backup and unchanged data, and then
  maps those settlements.
- **Documents** (`maps/migrate.rs`): every stored map document has
  `map_format`. Newer formats are refused. Older ones are upgraded in memory
  by an ordered chain of `UPGRADES` steps, and saved in the new format the
  next time they are written. A format change means: bump
  `MAP_FORMAT_VERSION`, append a step, and add a test that reads the old
  format. Steps may restructure fields but never change coordinates or IDs.
- **Engine upgrades:** stored maps keep their engine ID and version. A newer
  engine draws only when the user regenerates. Regeneration keeps the seed
  (unless asked otherwise), increases the revision, and carries the
  metadata over. Details and placements for buildings that no longer exist
  are kept, orphaned, never re-attached by guesswork. An Inspector should
  offer to reattach or discard them.
- **Before release:** migration 0002 is not yet released and may still
  change on this branch. Once a release ships it, it is frozen. A
  development build run against a real user database applies 0002 (after
  the automatic backup), so test against copies.

## 9. Interfaces

- **Core:**
  - `AppService::maps_get(id)` returns a `SettlementMapsView`: status
    (`not_generated`, `current`, `stale`), layout, any problem stopping
    generation, the engine new maps would use, the stored set, the numbered
    markers and legend of each map, and `planned`: every map the settlement
    gets (for a City or Capital, one per recorded district, from the
    snapshot), each marked `generated` and `stale`.
  - `maps_generate(id, options)`, `maps_generate_with(engine, id, options)`
    and `maps_delete(id)`. `options.only` draws a single map (one district)
    and keeps the other saved maps exactly as they are. Each map also stores
    its own `inputs_fingerprint`, so staleness is reported per map.
- **GUI:** Tauri commands `maps_get`, `maps_generate`, `maps_delete`, the
  `maps_building*` commands (§12) and the `maps_location_*` commands (§13).
  `api.ts` has `maps.get`, `maps.generate` and `maps.remove`.
  `app/src/maps/types.ts` mirrors every type. A test fails if it misses a
  field the core sends.
- **Terminal:** Maps and Building screens over the same `AppService`
  methods (§15).

## 10. Implementation checklist

**Phase 1: architecture** ✅

- [x] Branch from a release tag; rebased to v0.1.5 at the user's request
- [x] Existing architecture documented (§2)
- [x] settlemaker investigated: versions, API, browser use, IDs, SVG/GeoJSON
      matching, performance, limits (§3)
- [x] Licensing findings and unresolved decisions (§4)
- [x] Typed interfaces in Rust and TypeScript (§5)
- [x] Seeds, IDs, map/geometry/engine versions (§6)
- [x] Schema and document versioning with migration strategy (§8)

**Phase 2: engine integration** ✅

- [x] Engine-neutral `MapEngineAdapter`, validation and panic isolation
- [x] Original built-in engine: buildings, streets, walls/gates, coast/river/lake,
      bridges, trees, fields, squares, fitted bounds
- [x] Inputs from the saved snapshot; data-driven defaults; manual overrides
- [x] One map for Trading Post, Village and Town; one per recorded district
      for City and Capital
- [x] Persistence (migration 0002), atomic saves, staleness, deletion
- [x] Tests: determinism, IDs, non-empty geometry, every Size, bounds,
      districts, small and large inputs, failure safety, upgrade path

**Phase 3: Map Explorer GUI** ✅

- [x] Map tab on saved settlements (and an explanatory tab on unsaved ones)
- [x] District tabs from the snapshot's districts; each district generated on
      its own (`options.only`); a view and a selection kept per district
- [x] Canvas, toolbar, collapsible Location Inspector, searchable collapsible
      Legend, searchable building directory; resizes without breaking
- [x] Wheel zoom about the pointer, drag pan, Fit map, Reset view, keyboard
      pan and zoom, click selection by footprint, hover, Esc clears
- [x] Optional district-area and marker layers
- [x] Rendering chosen by profiling (§11); spatial-index hit testing
- [x] Empty states with Generate Map; never generated automatically
- [x] Both themes; tests for zoom, pan, selection, districts, navigation and
      rendering cost

**Phase 4: building details and Location Inspector** ✅

- [x] Every building starts ungenerated; drawing a map creates no details
- [x] First open generates and stores details once; later opens read them
- [x] States: ungenerated, generated, customised; provenance per field
- [x] Content tables in `content/17-buildings/` (provisional, editable)
- [x] Edit, Regenerate (keeps edits unless explicitly replaced), Reset,
      Locate on map; Make Notable Location wired but disabled
- [x] One row per building (migration 0003), version-checked writes
- [x] Tests: once-only generation, no rerolls, customisation, restart,
      protection, separate records, district isolation, blank fields

**Phase 5: Notable Locations, markers and legend** ✅

- [x] Directory of the settlement's own locations (from the snapshot) and
      custom ones, with stable IDs, names, notes and types
- [x] Make Notable Location dialog: search, choose, custom entry, confirm
- [x] One location per building and one building per location; moving and
      replacing only by explicit choice (and a database unique index)
- [x] Persistent, settlement-wide marker numbers; gaps, never renumbering
- [x] Pin markers (round for locations, diamond for tokens); legend and
      directory entries open the building, switching district if needed
- [x] Unassign, Reassign (choose a building), rename and notes
- [x] Tests: assignment, removal, reassignment, persistence, legend and
      marker agreement, district navigation, duplicate prevention

**Phase 6: district maps** ✅

- [x] Trading Post, Village, Town: one whole map; City, Capital: one map per
      recorded district, drawn on demand, never rerolling the settlement
- [x] District names (display only; IDs, maps and records unchanged)
- [x] Delete one district's map, warning what goes with it
- [x] Points of interest: add, name, move, delete; numbers shared with
      Notable Locations
- [x] Saved view per map; district search; settlement-wide directory of
      locations and points, by district, navigating to the right district
- [x] Tests: settlement types, multi-district persistence, directory
      navigation, stable references, lazy loading, district switching

**Phase 7: persistence, exports, compatibility, terminal** ✅

- [x] Every map record stored in versioned tables and documents (§15)
- [x] A real v0.1.1 database opens, keeps every settlement, and can be
      mapped later without regenerating anything
- [x] Damaged optional map data is reported and can be discarded; the
      settlement always opens
- [x] PNG and SVG per map or per district (with optional markers and
      legend), structured map data (JSON), settlement export and import
- [x] Terminal Maps and Building screens: generate, list, inspect, edit,
      locations, points, export; import from Saved Settlements
- [x] Tests: old saves, new saves, reloads, imports, exports, Unicode names,
      multi-district records, terminal and GUI consistency

**Next phases**

- [ ] **Owner to supply the real building content.** The building-detail
      tables in `content/17-buildings/` are provisional placeholders; replace
      them (and the "Provisional content" notes) when the content arrives.
- [ ] Inspector: offer to reattach or discard details orphaned by a redraw.

- [ ] **Owner decisions:** the app's licence; settlemaker yes or no (§4).
      District names and types, if wanted, need content tables (none exist).
- [ ] Automatic first placement of Notable Locations (today every placement
      is the user's). The settlement-level Tavern of a City or Capital may be
      placed on any district's map.
- [ ] Search (buildings, locations, tokens) in the core.
- [ ] GUI: a folder picker for exports and a "show in folder" action (both
      need a Tauri plugin; exports go to the default folder for now).
- [ ] Terminal interface: free-text notes for locations and points, and
      notes longer than one line (the text modal is single-line).
- [ ] Regeneration UX: the GUI confirms and shows staleness per district;
      orphaned metadata still needs handling once metadata exists.
- [ ] Parameter editor for overrides (all maps and per map).
- [ ] Freeze migrations 0002–0005, `MAP_FORMAT_VERSION` 1, settlement export
      format 1 and map data format 1 at release; release notes; Credits if
      any third-party engine or artwork is adopted (resvg, used for PNG, is
      MIT OR Apache-2.0).

## 11. Map Explorer (GUI)

Source: `app/src/maps/` (`MapExplorer.tsx`, `MapCanvas.tsx`, `MapPanels.tsx`,
`viewport.ts`, `spatial.ts`, `paths.ts`), `components/SettlementTabs.tsx`,
`styles/map.css`, and the `--map-*` tokens in `styles/tokens.css`.

**Where it is.** Saved Settlements has **Settlement** and **Map** tabs. The
Map tab loads the stored maps with `maps_get`. While a map is shown, the
screen uses the full window width. A just-generated, unsaved settlement has a
Map tab that explains maps belong to saved settlements. Nothing is ever
generated automatically: only the Generate and Redraw buttons call
`maps_generate`.

**Layout.**

- District tabs (City and Capital only), built from `planned`, so names and
  IDs are the snapshot's own. A district without a map is marked "No map"
  and has its own Generate button. "Changed" marks a stale map.
- The map toolbar: zoom out and in (with %), Fit map, Reset view, the
  District area and Markers layer toggles, and a button that hides the side
  panel.
- The canvas, with a scale bar.
- A right-hand column of collapsible panels: the **Location Inspector**, the
  **Legend** (searchable, listing only features the map has, plus numbered
  locations), and the **Building directory** (searchable listbox).
- Below 1,100 px wide, the column moves under the map. The canvas size comes
  from a `ResizeObserver`. A map the user has not moved is refitted on
  resize; otherwise the view stays centred.

**Interaction.**

| Input                     | Effect                                                                                         |
| ------------------------- | ---------------------------------------------------------------------------------------------- |
| Wheel                     | Zoom about the pointer (limits ½× to 24× the whole-map fit)                                    |
| Drag                      | Pan (more than 4 px of movement counts as a drag, not a click)                                 |
| Click                     | Select the building under the pointer; clicking empty ground clears the selection              |
| Hover                     | Highlight the building under the pointer                                                       |
| Arrows / + − / 0 / Home   | With the map focused: pan, zoom, fit the whole map, reset to the default view                  |
| Esc                       | Clears the selection; with nothing selected, Esc goes back as on every screen                  |
| Directory: ↑ ↓ Home End ⏎ | Move through buildings; Enter selects and centres the map. The way to select without a pointer |

**Views per district.** Each map keeps its own viewport and selection in
memory. Switching districts restores them, and nothing is regenerated.
"Reset view" fits the built area (the default view); "Fit map" shows the
whole frame.

**Rendering, chosen by profiling.** Measured in headless Edge on real
generated geometry. Each figure is the mean frame time while panning and
zooming continuously, against a 16.7 ms budget:

| Buildings                | SVG, one element per building | SVG, merged layers | Canvas 2D |
| ------------------------ | ----------------------------- | ------------------ | --------- |
| 170 (Capital district)   | 6.0 ms                        | 4.2 ms             | 4.2 ms    |
| 600 (the core's maximum) | 4.8 ms (675 nodes)            | 4.4 ms (4 nodes)   | 4.2 ms    |
| 2,400 (4× the maximum)   | 10.2 ms                       | 10.0 ms            | 6.5 ms    |

Up to the 600-building limit, merged SVG layers match Canvas; Canvas only
pulls ahead well beyond what the core can produce. Merged SVG was chosen.
It themes directly from CSS variables, stays crisp for a future SVG export,
and can be tested in jsdom.

- **Static layers.** Each layer (buildings, each street kind, water, walls,
  fields, trees and so on) is one `<path>` whose data is built once per map
  and cached (`paths.ts`). It is rendered by a memoised component. A
  600-building map is under 30 SVG elements.
- **Overlays.** Pan and zoom change one `transform`. Hover and selection
  each draw one extra outline. Markers are drawn in screen space at a
  constant size.
- **Measured in the real build:** 5.6–5.8 ms per frame over 60 wheel zooms
  of a 600-building map.
- **Not used:** a React component per building, progressive rendering and
  tile caching. None is needed at this size.

**Hit testing** (`spatial.ts`) uses the stored footprints, never the
artwork. A uniform 24 m grid maps each cell to the buildings whose bounding
box touches it. A query tests at most a few polygons (even-odd test, as in
the core), and the grid is built once per map. Tests check it against
testing every building, and that 10,000 queries stay fast.

**State separation.**

- Geometry comes from the stored map and is never mutated.
- Viewport, hover and selection are local UI state.
- Inspector content is derived from the selected building's ID.
- Building details are fetched on selection only (never on hover) and cached
  by building ID in the Map Explorer. Unsaved drafts are kept per building.
- Notable Locations are placed only by the user (§13).

## 12. Building details and the Location Inspector

Source: `maps/details.rs`, `db/maps.rs` (building rows),
`app/src/maps/LocationInspector.tsx`. Content: `content/17-buildings/`
(provisional; see [CONTENT.md](CONTENT.md)).

**Lazy generation.**

- Drawing a map creates geometry and building IDs only. A building with no
  stored record is **ungenerated**.
- `AppService::maps_building` is called when a building is selected. The
  first call rolls the detail tables for that building alone, under the
  settlement's tags (from the map set's inputs), and stores the record. Every
  later call returns the stored record; nothing is rerolled.
- The insert is `ON CONFLICT DO NOTHING` and returns the stored row, so a
  building opened in the GUI and the terminal at once is still generated once.

**States and provenance.**

| State         | Meaning                                                       |
| ------------- | ------------------------------------------------------------- |
| `ungenerated` | No record.                                                    |
| `generated`   | Record stored; no user edits.                                 |
| `customised`  | At least one user value (a deliberate blank counts) or notes. |

The state is derived from the record, never stored separately. The record
keeps two things apart:

- `generated`: each field with its `source`: `table` (the whole roll, every
  die), `inferred` (the address, from the building number and street) or
  `not_rolled` (for example no occupants for a storehouse, or a deleted
  table).
- `edits`: the user's values. `{ "value": null }` is a field left blank on
  purpose; an absent field shows its generated value. Notes are user-only.

The view sends each field's current value, its `origin` (`table`,
`inferred`, `not_rolled` or `user`) and the generated value.

**Changes.** Each writes one building's row and nothing else.

| Call                       | Effect                                                                                     |
| -------------------------- | ------------------------------------------------------------------------------------------ |
| `maps_building_edit`       | Replaces the user's edits after validation (lengths, single lines, occupants 0 to 10,000). |
| `maps_building_regenerate` | Rerolls `generated`. Edits are kept unless `discard_edits`; notes are always kept.         |
| `maps_building_reset`      | Drops the field edits, giving back the generated values. Notes are kept.                   |

Every change names the `version` it was made from. A stale version is
refused ("changed elsewhere"), so one interface never overwrites the other's
newer edit. An unchanged result is not written.

**Storage.** Migration 0003 adds `settlement_map_buildings` (primary key
`settlement_id, building_id`), which cascades with the settlement. Reading a
row checks that its document names the same building. A redraw gives
buildings new IDs, so old rows are kept but never listed or reattached.
`maps_delete` removes them.

**Inspector (GUI).**

- It shows the building number, district or map, name or address, type,
  condition, occupancy, occupants, size, street, linked Notable Location and
  description, each with an origin badge, plus notes. "Sources and record"
  lists every roll, the building ID, dates and version.
- Actions: Edit details, Locate on map, Regenerate…, Reset…, Make Notable
  Location (disabled until placement exists), Clear selection.
- Editing happens in a draft, and one Save makes one write. "Use generated"
  gives a field back. Cancel or Esc closes the form, asking first if there
  are unsaved changes. Esc never clears the map selection while editing.
  After a save, Undo restores the previous edits.
- Regenerate always asks first. If the user has changed fields, replacing
  them needs an extra checkbox, and the button becomes a danger button.
  Reset asks first, and is disabled when nothing was changed.
- The building directory lists generated buildings by name and type.

## 13. Notable Locations on the map

Source: `maps/directory.rs`, `db/maps.rs` (location rows),
`app/src/maps/NotableLocations.tsx`, the Legend in `MapPanels.tsx` and the
pins in `MapCanvas.tsx`.

**The directory.** `SettlementMapsView.locations` lists every Notable
Location of the settlement:

- **Settlement locations** come from the saved snapshot: every location the
  Notable Locations stage rolled (`notable-location-NN`), and the Bare
  Minimum Buildings Tavern. The snapshot is never changed. What the user sets
  (name, notes, marker, building) is stored against the snapshot's own ID.
  The future Notable Locations generator connects by producing the same IDs.
- **Custom locations** (`custom-location-N`) are made by the user, with a
  name and a free-text type (for example Castle).
- Every entry has a stable ID, a display name (the user's, or by default the
  subtype or type), a type, the district the settlement records it in, its
  marker number and where it stands.
- A settlement location that the settlement no longer has (it was edited
  since) is kept and marked `removed` if the user set anything on it. It
  cannot be placed again.

**Rules.**

| Rule                      | How                                                                                                                                                        |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| One building per location | Placing a location that stands elsewhere is refused unless `move_location`.                                                                                |
| One location per building | Placing on a taken building is refused unless `replace_occupant`. The location taken off is kept, unplaced. A unique index also enforces this.             |
| District                  | A location the settlement records in a district stands only on that district's map. The guaranteed Tavern and custom ones go anywhere.                     |
| Marker numbers            | Issued once per location, the first time it is placed, from one sequence per settlement. They are kept when unplaced and never reused, so gaps are normal. |
| Building details          | Never touched by placing, moving or unplacing.                                                                                                             |
| Redrawn maps              | The placement keeps its old building ID and is shown as missing. It is not drawn, and can be placed again without "move".                                  |
| Deleted maps              | Placements are cleared (a later first drawing reuses building IDs). Names, notes and numbers are kept.                                                     |

**Storage.** Migration 0004 adds `settlement_locations` (one row per location
with a record; `building_id` column with a unique index per settlement) and
`settlement_location_sequences` (next marker and custom numbers, which only
grow). Every change runs in one `IMMEDIATE` transaction: read, change, then
write only the rows that changed, each with its version increased.

**Calls.** `maps_location_assign`, `maps_location_unassign`,
`maps_location_update` (name, notes, a custom location's type; version
checked), `maps_location_create` (optionally placed, all or nothing) and
`maps_location_delete` (custom only). Each returns the new
`SettlementMapsView`.

**GUI.**

- **Make Notable Location** (Inspector) opens a dialog. It lists every
  location, searchable by name, type or district, with its number and
  status ("Not placed", "On Building 3, District 1", "Belongs to District
  2", disabled), plus **Custom entry…**. Nothing changes until it is
  confirmed. A move or replace is spelled out first, and the button becomes
  "Move here" or "Replace".
- **Inspector**, for a building with a location: its number, name, type and
  notes, with Edit location (rename and notes), Reassign… (then choose a
  building on the map, then confirm; Esc cancels) and Unassign… (confirmed).
- **Markers** are pins drawn in screen space at a constant size. The tip
  marks the building's centre and the numbered head stands above it, so
  the building stays visible. Notable Locations have round accent heads;
  tokens have diamond heads. Clicking a pin selects its building.
- **Legend**: this map's numbered entries with name, type and district,
  searchable. Clicking one selects the building, zooms in to at least twice
  the default view, centres it and opens the Inspector.
- **Notable Locations panel**: the settlement-wide directory, grouped by
  district for Cities and Capitals. Clicking a placed location switches to
  its district first.

## 14. District maps

Source: `maps/directory.rs` (names, points of interest), `db/maps.rs`,
`app/src/maps/Districts.tsx` and `MapExplorer.tsx`.

**Which maps.** Trading Posts, Villages and Towns have one complete map
(`map-settlement`). Cities and Capitals have one map per district the
Notable Locations stage recorded (`map-district-N`), and no whole-city map.
There are no other map scales.

**What each district has.**

| Item                | Where                                                                          |
| ------------------- | ------------------------------------------------------------------------------ |
| Stable ID, parent   | `district-N` from the snapshot; the map's `settlement_id`                      |
| Name                | The snapshot's "District N", or the user's (`settlement_district_names`)       |
| Map ID              | `map-district-N`                                                               |
| Seed, inputs        | Derived from the set seed and map ID; inputs copied from the snapshot (§6, §7) |
| Geometry, buildings | The stored map (§8); building details per building (§12)                       |
| Notable Locations   | The directory, placed on its buildings (§13)                                   |
| Points of interest  | `settlement_map_pois`, numbered from the location sequence                     |
| Saved view          | `settlement_map_views`: centre, and zoom relative to the default view          |

**Rules.**

- Districts are drawn one at a time, on demand. Drawing one never touches
  another, never rerolls a settlement table and never generates building
  details.
- **Geography and roads.** Every district map takes the settlement's own
  facts (water, ground, roads leaving, irregularity) from its tags, so the
  districts of a coastal city all have coast. That is as far as the built-in
  engine goes. District maps are separate local views: no continuous city
  layout, no shared edges and no connections between districts are invented.
- **Renaming** stores a display name only. IDs never change, so building
  records, placements, points and views keep working. Every view (tabs,
  legend, directory, Inspector, messages) shows the new name; the stored
  maps keep the label they were drawn with. A blank name gives back the
  default.
- **Deleting one district's map** (`maps_delete_map`) runs in one
  transaction. It deletes the map, its building details, its points of
  interest and its saved view, and takes its Notable Locations off (kept,
  with their numbers). The district, its name, the other maps and the set's
  seed and revision are kept, so drawing it again gives new building IDs.
  `PlannedMapView.annotations` counts what would go, and the GUI says so in
  the confirmation. The settlement itself is never deleted here; deleting a
  settlement keeps the app's existing rules (favourites are refused).
- **Points of interest** sit at a position on one map. Each takes the next
  number of the settlement-wide sequence it shares with Notable Locations,
  and `poi-N` as its ID. Neither is reused. Moving one keeps its number.
  They are deleted with their map.
- **Saved views** are written after the view stays still for 600 ms, never
  on every movement. A centre outside the map, or an absurd zoom, is
  refused.

**Performance.** Measured: the largest Capital (5 districts) sends about
250 KB of maps to the GUI over local IPC. Only the district on screen is
rendered, and no building details are generated until a building is opened.
Splitting the maps into one request per district was therefore not needed.

**GUI.**

- **District bar** (Cities and Capitals): search districts (Enter or a match
  switches), the district's name with Rename… and, when drawn, Delete map…
  (confirmed, with the counts). **Settlement directory** opens the
  directory for the whole settlement.
- **Add point** (toolbar), then click the map and name the point. A point's
  diamond pin selects it. The Inspector shows its number, district and notes,
  with Edit point, Locate on map, Move… (click the new position) and Delete…
  (confirmed).
- **Directory** panel: every Notable Location and point of interest,
  searchable. Cities and Capitals group it by district, and can narrow it to
  the district on screen. Clicking an entry switches district, zooms to and
  centres it, and selects it.
- Each district opens where it was last left, also after a restart.

## 15. Persistence, exports and the terminal

**What is stored, where.** Everything is in the application's SQLite
database. That is the same store saved settlements use, migrated
transactionally with a verified backup first.

| Data                                          | Stored in                                  | Version                         |
| --------------------------------------------- | ------------------------------------------ | ------------------------------- |
| Map set: layout, seed, revision, inputs       | `settlement_map_sets`                      | schema 2, `map_format`          |
| Each map: geometry, buildings, engine version | `settlement_maps`                          | schema 2, `map_format`          |
| Building details and their state              | `settlement_map_buildings`                 | schema 3, `map_format`, version |
| Notable Location directory, marker numbers    | `settlement_locations`, `…_sequences`      | schema 4, `map_format`, version |
| Points of interest (coordinates, numbers)     | `settlement_map_pois`                      | schema 5, `map_format`          |
| District names, saved views                   | `settlement_district_names`, `…_map_views` | schema 5                        |

- No pictures are stored: exports are drawn from the geometry when asked.
- Engine updates never regenerate stored geometry. A map keeps the engine
  version it was drawn with until the user redraws it.

**Old saves.** `tests/fixtures/v0.1.1.sqlite3` was written by the released
v0.1.1 code itself, with a Town, a Unicode-named City and a Village.
`tests/compatibility.rs` opens a copy with this build. The schema upgrades to
the latest (with a backup), every settlement reads back unchanged, and none
has maps. The Town and Village can then be mapped. The City is mapped by
district, but a v0.1.1 City recorded no districts, so it says so instead of
being given invented ones.

**Damaged data.** A building, location or point row that cannot be read
(damaged, or from a newer version) is left out and listed in
`SettlementMapsView.damaged`. So is a map set that cannot be read. The
settlement, its snapshot and everything readable still open. The recovery is
`maps_discard_damaged` ("Discard damaged data…" in the GUI, confirmed).
A settlement export is refused until damaged data is discarded, so an export
is always complete.

**Exports** (`AppService::maps_export`, `maps/render.rs`, `maps/export.rs`):

- **SVG**: one map, drawn from stored geometry in a light print palette.
  Numbered pins and a legend panel are optional. Text is escaped and keeps
  any Unicode.
- **PNG**: the same drawing, rasterised by resvg with the system fonts, up to
  64 megapixels.
- **Map data** (`*.map.json`, `settlement-generator.map-data` format 1): the
  exported maps with their geometry, stored building details, directory
  entries, points of interest and legends.
- **One map or all.** A City's or Capital's districts export one file each
  ("Settlement - District.png"). No joined city picture is made.
- Files go to `Documents/Settlement Generator` by default
  (`SETTLEMENT_GENERATOR_EXPORT_DIR` overrides it). Each is written to a
  temporary file, flushed and renamed into place, so an interrupted export
  never leaves a partial file. File names keep Unicode and avoid characters
  and names Windows refuses.

**Settlement export and import** (`archive.rs`, `*.settlement.json`,
`settlement-generator.settlement` format 1):

- One settlement with its snapshot and every map record: the map set,
  building details, the location directory with marker sequences, points,
  district names and views.
- Import adds a **new** settlement (new ID) in **one transaction**, rewriting
  the owning settlement ID inside the maps. It never overwrites anything.
- Refused with a clear message, and nothing stored, when the file:
  - is not JSON, or not this kind of file;
  - is from a newer export, snapshot or map format;
  - fails consistency checks (duplicate maps or buildings, two locations on
    one building, marker numbers not below the next one to issue);
  - has a name that is not valid.

**GUI.** Map toolbar → **Export…**: formats (PNG, SVG, map data), this
district or every drawn district, markers and legend. It shows the files
written. Saved Settlements: **Export** writes the settlement file;
**Import settlement…** reads one (a file input, so no extra plugin).

**Terminal** (`settlement-cli/src/tui/screens/maps.rs`): Saved Settlements →
**Maps**, and `i` to import a file by path.

| Operation                       | Where                                                                                                             |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| List maps and districts         | Maps screen: each map with its status and building count                                                          |
| Generate a map or district      | Generate / Redraw (confirmed)                                                                                     |
| List buildings and their IDs    | Buildings…: searchable by number, name, type or ID                                                                |
| Inspect a building              | Building screen: every field with its origin (rolled, inferred, edited)                                           |
| Generate details                | Opening a building (once; never rerolled)                                                                         |
| Edit building information       | Edit a field…: empty means blank on purpose, `=` gives back the generated value                                   |
| Regenerate / Reset              | Regenerate… (keep or replace your changes) / Reset… (confirmed)                                                   |
| Notable Locations               | Locations… (place, move or replace with confirmation, unassign, rename), and Make Notable Location… on a building |
| Points of interest              | Points…: add by `x y` in metres from the map's centre, rename, move, delete                                       |
| Rename or delete a district map | Rename… / Delete map (counts shown, confirmed)                                                                    |
| Export                          | Export…: PNG, SVG, map data, all three, or every district                                                         |

The terminal never draws polygons. It shows IDs, counts, coordinates and the
numbered legend. Because both interfaces use the same `AppService` and
database, a change made in one is seen by the other; the terminal tests
check this through the service the GUI uses.
