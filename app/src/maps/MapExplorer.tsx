import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { maps } from "../api";
import { ConfirmBar } from "../components/generation";
import { MapCanvas, type CanvasCommand } from "./MapCanvas";
import { LocationInspector, type Draft } from "./LocationInspector";
import { BuildingDirectory, Legend, Panel, buildingLabel } from "./MapPanels";
import { DistrictBar, PoiForm, PoiInspector } from "./Districts";
import { ExportDialog } from "./MapExport";
import {
  LocationDialog,
  LocationDirectory,
  isPlaced,
  type DirectoryScope,
  type LocationChoice,
} from "./NotableLocations";
import type {
  BuildingDetailsView,
  BuildingEdits,
  BuildingSummary,
  LocationEdit,
  LocationEntry,
  MapBuilding,
  PoiEntry,
  Point,
  SavedView,
  MapGenerationOptions,
  PlannedMapView,
  SettlementMap,
  SettlementMapsView,
} from "./types";
import {
  ZOOM_STEP,
  boundsOf,
  centreOn,
  fitBounds,
  toMap,
  zoomAt,
  zoomLimits,
  type Size,
  type Viewport,
} from "./viewport";

/** Canvas size before the first measurement (and in tests). */
const DEFAULT_SIZE: Size = { width: 800, height: 520 };

/** The default ("reset") view: the built area, with some countryside. */
function homeView(map: SettlementMap, size: Size): Viewport {
  const built = boundsOf(
    map.geometry.buildings.map((b) => b.footprint),
    25,
  );
  return fitBounds(built ?? map.geometry.bounds, size);
}

function fitView(map: SettlementMap, size: Size): Viewport {
  return fitBounds(map.geometry.bounds, size);
}

/** How long a view must stay still before it is remembered (ms). */
export const VIEW_SAVE_DELAY = 600;

/** A saved view (centre, zoom relative to the default) on this canvas. */
function fromSaved(map: SettlementMap, saved: SavedView, size: Size): Viewport {
  const home = homeView(map, size);
  const limits = zoomLimits(map.geometry.bounds, size);
  const k = Math.min(limits.max, Math.max(limits.min, home.k * saved.zoom));
  return centreOn({ k, tx: 0, ty: 0 }, saved.centre, size);
}

/** A round scale-bar length (metres) close to `target`. */
function niceLength(target: number): number {
  const p = 10 ** Math.floor(Math.log10(target));
  return [5, 2, 1].map((m) => m * p).find((v) => v <= target) ?? p;
}

/**
 * The Map tab of a saved settlement: its one map (Trading Post, Village,
 * Town) or one map per district (City, Capital). Maps are read back as
 * stored; nothing is generated unless the user asks. Selecting a building
 * never redraws or regenerates the map.
 */
export function MapExplorer({
  settlementId,
  settlementName,
}: {
  settlementId: string;
  settlementName: string;
}) {
  const [view, setView] = useState<SettlementMapsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selection, setSelection] = useState<Record<string, string | null>>({});
  const [viewport, setViewport] = useState<Viewport | null>(null);
  const [size, setSize] = useState<Size>(DEFAULT_SIZE);
  const [sideOpen, setSideOpen] = useState(true);
  const [panels, setPanels] = useState({
    inspector: true,
    legend: true,
    locations: true,
    directory: false,
  });
  // Notable Locations: the Make Notable Location dialog, the "choose a
  // building" mode of Reassign, and a building to show once its map is on
  // screen (after switching district from the legend or directory).
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [locationBusy, setLocationBusy] = useState(false);
  const [moving, setMoving] = useState<LocationEntry | null>(null);
  const [moveTarget, setMoveTarget] = useState<MapBuilding | null>(null);
  const [focus, setFocus] = useState<
    { mapId: string; buildingId: string } | { mapId: string; poi: PoiEntry } | null
  >(null);
  // Points of interest: the selected one, placing a new one (then naming
  // it) or moving one, and the directory's scope.
  const [poiId, setPoiId] = useState<string | null>(null);
  const [placing, setPlacing] = useState<{ kind: "add" } | { kind: "move"; poi: PoiEntry } | null>(
    null,
  );
  const [newPoiAt, setNewPoiAt] = useState<{ mapId: string; point: Point } | null>(null);
  const [scope, setScope] = useState<DirectoryScope>("settlement");
  const [exportOpen, setExportOpen] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const directorySearch = useRef<HTMLInputElement>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [layers, setLayers] = useState({ district: true, markers: true });
  const [confirmRegenerate, setConfirmRegenerate] = useState(false);
  // Building details by building ID: fetched (and, the first time, generated)
  // when a building is selected, then kept so reselecting costs nothing.
  const [details, setDetails] = useState<Record<string, BuildingDetailsView>>({});
  const [detailErrors, setDetailErrors] = useState<Record<string, string>>({});
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [retry, setRetry] = useState(0);
  const inFlight = useRef(new Set<string>());
  const viewports = useRef(new Map<string, Viewport>());
  const toolbarRef = useRef<HTMLDivElement>(null);
  const describedBy = useId();
  const tabPanelId = useId();

  useEffect(() => {
    let live = true;
    maps.get(settlementId).then(
      (v) => live && setView(v),
      (e: unknown) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [settlementId]);

  // The active map: kept if still planned, else the first drawn, else the first.
  const planned = useMemo(() => view?.planned ?? [], [view]);
  useEffect(() => {
    if (planned.length === 0) return;
    if (!planned.some((p) => p.id === activeId)) {
      setActiveId((planned.find((p) => p.generated) ?? planned[0]).id);
    }
  }, [planned, activeId]);

  const activeMap = view?.maps?.maps.find((m) => m.id === activeId) ?? null;
  const activePlan = planned.find((p) => p.id === activeId) ?? null;
  const selectedId = activeId ? (selection[activeId] ?? null) : null;
  const selected = activeMap?.geometry.buildings.find((b) => b.id === selectedId) ?? null;
  const legend = view?.legends?.find((l) => l.map_id === activeId);
  const summaries = useMemo(
    () => new Map((view?.buildings ?? []).map((b) => [b.building_id, b])),
    [view?.buildings],
  );
  const selectedDetails = selectedId ? (details[selectedId] ?? null) : null;
  const locations = useMemo(() => view?.locations ?? [], [view?.locations]);
  /** The Notable Location standing on a building of the active map. */
  const locationOn = (buildingId: string | null) =>
    buildingId
      ? locations.find(
          (l) =>
            isPlaced(l) &&
            l.placement.building_id === buildingId &&
            l.placement.map_id === activeId,
        )
      : undefined;
  const selectedLocation = locationOn(selectedId);
  const pois = useMemo(() => view?.pois ?? [], [view?.pois]);
  const selectedPoi = pois.find((p) => p.id === poiId && p.map_id === activeId) ?? null;
  const savedViews = useRef(new Map<string, SavedView>());
  for (const v of view?.views ?? []) {
    if (!viewports.current.has(v.map_id)) savedViews.current.set(v.map_id, v);
  }

  /** Keeps fetched details, and the summary the directory lists. */
  const storeDetails = useCallback((d: BuildingDetailsView) => {
    setDetails((all) => ({ ...all, [d.building_id]: d }));
    setDetailErrors(({ [d.building_id]: _, ...rest }) => rest);
    const summary: BuildingSummary = {
      map_id: d.map_id,
      building_id: d.building_id,
      state: d.state,
      name: d.name.value,
      building_type: d.building_type.value,
    };
    setView((v) =>
      v
        ? {
            ...v,
            buildings: [
              ...(v.buildings ?? []).filter((b) => b.building_id !== d.building_id),
              summary,
            ],
          }
        : v,
    );
  }, []);

  // Selecting a building opens its details. The core generates them the
  // first time and returns the stored ones after that; only this building
  // is ever generated. Hovering never does.
  useEffect(() => {
    if (!activeMap || !selectedId || details[selectedId] || detailErrors[selectedId]) return;
    if (inFlight.current.has(selectedId)) return;
    inFlight.current.add(selectedId);
    const buildingId = selectedId;
    maps
      .building(settlementId, activeMap.id, buildingId)
      .then(storeDetails, (e: unknown) =>
        setDetailErrors((all) => ({ ...all, [buildingId]: String(e) })),
      )
      .finally(() => inFlight.current.delete(buildingId));
  }, [activeMap, selectedId, details, detailErrors, settlementId, storeDetails, retry]);

  /** Runs a change to the selected building; on failure shows why and reloads it. */
  const changeDetails = async (
    action: (d: BuildingDetailsView) => Promise<BuildingDetailsView>,
  ): Promise<boolean> => {
    if (!selectedDetails) return false;
    const d = selectedDetails;
    try {
      storeDetails(await action(d));
      return true;
    } catch (e) {
      setError(String(e));
      // The stored details may have changed elsewhere: read them again.
      maps.building(settlementId, d.map_id, d.building_id).then(storeDetails, () => {});
      return false;
    }
  };

  // Measure the canvas area. The stage appears only once the maps have
  // loaded, so it is observed through a callback ref. A map the user has
  // not moved yet is fitted again; otherwise the view stays centred.
  const sizeRef = useRef(size);
  const activeRef = useRef(activeMap);
  activeRef.current = activeMap;
  const observer = useRef<ResizeObserver | null>(null);
  const stageRef = useCallback((element: HTMLDivElement | null) => {
    observer.current?.disconnect();
    observer.current = null;
    if (!element || typeof ResizeObserver === "undefined") return;
    observer.current = new ResizeObserver(([entry]) => {
      const width = Math.round(entry.contentRect.width);
      const height = Math.round(entry.contentRect.height);
      const old = sizeRef.current;
      if (width < 50 || height < 50 || (old.width === width && old.height === height)) return;
      const next = { width, height };
      sizeRef.current = next;
      setSize(next);
      const map = activeRef.current;
      if (map && !viewports.current.has(map.id)) setViewport(homeView(map, next));
      else {
        const dx = (width - old.width) / 2;
        const dy = (height - old.height) / 2;
        setViewport((v) => (v ? { ...v, tx: v.tx + dx, ty: v.ty + dy } : v));
      }
    });
    observer.current.observe(element);
  }, []);

  // Each map keeps its own view: restored when it is shown again.
  useEffect(() => {
    if (!activeMap) {
      setViewport(null);
      return;
    }
    const saved = savedViews.current.get(activeMap.id);
    setViewport(
      viewports.current.get(activeMap.id) ??
        (saved ? fromSaved(activeMap, saved, sizeRef.current) : null) ??
        homeView(activeMap, sizeRef.current),
    );
    // `size` is read once per map: resizing keeps the current view.
  }, [activeMap]);

  // Showing a building from the legend or the directory: once its map is
  // active, zoom in enough to see it, centre it and select it.
  useEffect(() => {
    if (!focus || !activeMap || activeMap.id !== focus.mapId) return;
    setFocus(null);
    const building =
      "buildingId" in focus
        ? activeMap.geometry.buildings.find((b) => b.id === focus.buildingId)
        : undefined;
    const point = building?.centroid ?? ("poi" in focus ? focus.poi.position : undefined);
    if (!point) return;
    const size = sizeRef.current;
    const home = homeView(activeMap, size);
    const limits = zoomLimits(activeMap.geometry.bounds, size);
    const base = viewports.current.get(activeMap.id) ?? home;
    const k = Math.min(limits.max, Math.max(base.k, home.k * 2));
    const next = centreOn({ ...base, k }, point, size);
    viewports.current.set(activeMap.id, next);
    setViewport(next);
    remember(activeMap, next);
    if (building) {
      setPoiId(null);
      setSelection((s) => ({ ...s, [activeMap.id]: building.id }));
    } else if ("poi" in focus) {
      setPoiId(focus.poi.id);
      setSelection((s) => ({ ...s, [activeMap.id]: null }));
    }
  }, [focus, activeMap]);

  /** Shows a building, switching district first if it is on another map. */
  const goTo = (mapId: string, buildingId: string) => {
    setFocus({ mapId, buildingId });
    setActiveId(mapId);
    setSideOpen(true);
    setPanels((p) => ({ ...p, inspector: true }));
  };

  /** Shows a point of interest, switching district first if needed. */
  const goToPoi = (poi: PoiEntry) => {
    setFocus({ mapId: poi.map_id, poi });
    setActiveId(poi.map_id);
    setSideOpen(true);
    setPanels((p) => ({ ...p, inspector: true }));
  };

  /**
   * Remembers where a map was left, once the view has stayed still for a
   * moment: one small write, never one per movement.
   */
  const remember = (map: SettlementMap, v: Viewport) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    const size = sizeRef.current;
    saveTimer.current = setTimeout(() => {
      const b = map.geometry.bounds;
      const [x, y] = toMap(v, size.width / 2, size.height / 2);
      const centre: Point = [
        Math.min(b.max_x, Math.max(b.min_x, x)),
        Math.min(b.max_y, Math.max(b.min_y, y)),
      ];
      const zoom = v.k / homeView(map, size).k;
      maps.saveView(settlementId, { map_id: map.id, centre, zoom }).catch(() => {});
    }, VIEW_SAVE_DELAY);
  };
  useEffect(
    () => () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    },
    [],
  );

  const updateViewport = (v: Viewport) => {
    if (activeId) viewports.current.set(activeId, v);
    setViewport(v);
    if (activeMap) remember(activeMap, v);
  };

  const select = useCallback(
    (building: MapBuilding | null) => {
      if (activeId) setSelection((s) => ({ ...s, [activeId]: building?.id ?? null }));
      if (building) setPoiId(null);
    },
    [activeId],
  );

  /** A click on the map while placing a point of interest. */
  const placeAt = async (point: Point) => {
    if (!placing || !activeMap) return;
    if (placing.kind === "add") {
      setNewPoiAt({ mapId: activeMap.id, point });
      setPlacing(null);
      setSideOpen(true);
      setPanels((p) => ({ ...p, inspector: true }));
      return;
    }
    const poi = placing.poi;
    setPlacing(null);
    const failure = await changeLocations(() => maps.movePoi(settlementId, poi.id, point));
    if (failure) setError(failure);
  };

  const addPoi = async (label: string, notes: string) => {
    if (!newPoiAt) return;
    const before = new Set(pois.map((p) => p.id));
    const at = newPoiAt;
    setLocationBusy(true);
    try {
      const next = await maps.addPoi(settlementId, at.mapId, at.point, label, notes);
      setView(next);
      setError(null);
      setNewPoiAt(null);
      const added = next.pois?.find((p) => !before.has(p.id));
      if (added) {
        setPoiId(added.id);
        setSelection((s) => ({ ...s, [at.mapId]: null }));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLocationBusy(false);
    }
  };

  /** Forgets cached details, drafts and selections of one map's buildings. */
  const forgetMap = (mapId: string) => {
    const onMap = (buildingId: string) =>
      buildingId.startsWith(`bld-${mapId.replace(/^map-/, "")}-r`);
    const keep = <T,>(all: Record<string, T>) =>
      Object.fromEntries(Object.entries(all).filter(([id]) => !onMap(id)));
    setDetails(keep);
    setDrafts(keep);
    setDetailErrors(keep);
    setSelection((s) => ({ ...s, [mapId]: null }));
    viewports.current.delete(mapId);
    savedViews.current.delete(mapId);
  };

  /** A building picked on the map: selected, or the target of a move. */
  const pick = (building: MapBuilding | null) => {
    if (moving && building) {
      const here = moving.placement?.building_id === building.id;
      setMoveTarget(here ? null : building);
    }
    select(building);
  };

  /** Runs a directory change; on failure says why and reloads the maps. */
  const changeLocations = async (
    action: () => Promise<SettlementMapsView>,
  ): Promise<string | null> => {
    setLocationBusy(true);
    try {
      setView(await action());
      setError(null);
      return null;
    } catch (e) {
      maps.get(settlementId).then(setView, () => {});
      return String(e);
    } finally {
      setLocationBusy(false);
    }
  };

  const chooseLocation = async (choice: LocationChoice) => {
    if (!activeMap || !selected) return;
    const target = { map_id: activeMap.id, building_id: selected.id };
    const failure = await changeLocations(() =>
      choice.kind === "existing"
        ? maps.assignLocation(settlementId, choice.id, target, choice.options)
        : maps.createLocation(settlementId, choice.edit, target, choice.options),
    );
    if (failure) setDialogError(failure);
    else {
      setDialogOpen(false);
      setDialogError(null);
    }
  };

  const confirmMove = async () => {
    if (!moving || !moveTarget || !activeMap) return;
    const occupied = !!locationOn(moveTarget.id);
    const failure = await changeLocations(() =>
      maps.assignLocation(
        settlementId,
        moving.id,
        { map_id: activeMap.id, building_id: moveTarget.id },
        { move_location: true, replace_occupant: occupied },
      ),
    );
    if (failure) setError(failure);
    setMoving(null);
    setMoveTarget(null);
  };

  const command = (c: CanvasCommand) => {
    if (!activeMap) return;
    updateViewport(c === "fit" ? fitView(activeMap, size) : homeView(activeMap, size));
  };

  const deleteMap = async (mapId: string) => {
    const failure = await changeLocations(() => maps.deleteMap(settlementId, mapId));
    if (failure) setError(failure);
    else {
      forgetMap(mapId);
      setPoiId(null);
    }
  };

  const renameDistrict = async (name: string | null): Promise<boolean> => {
    if (activePlan?.scope.kind !== "district") return false;
    const district = activePlan.scope.district.id;
    const failure = await changeLocations(() => maps.renameDistrict(settlementId, district, name));
    if (failure) setError(failure);
    return !failure;
  };

  /** Back to the settlement-wide directory, from any district. */
  const openDirectory = () => {
    setScope("settlement");
    setSideOpen(true);
    setPanels((p) => ({ ...p, locations: true }));
    requestAnimationFrame(() => directorySearch.current?.focus());
  };

  const generate = async (options: MapGenerationOptions, label: string) => {
    setBusy(label);
    setConfirmRegenerate(false);
    try {
      const next = await maps.generate(settlementId, options);
      setView(next);
      setError(null);
      // Regenerated buildings have new IDs: drop the old selection and the
      // details and drafts of the buildings that were replaced.
      if (options.only) setSelection((s) => ({ ...s, [options.only!]: null }));
      else setSelection({});
      const replaced = (mapId: string) => !options.only || options.only === mapId;
      const keep = <T extends { map_id: string }>(all: Record<string, T>) =>
        Object.fromEntries(Object.entries(all).filter(([, d]) => !replaced(d.map_id)));
      setDetails(keep);
      // Drafts and errors are keyed by building ID, which names its map.
      const onReplaced = (buildingId: string) =>
        !options.only || buildingId.startsWith(`bld-${options.only.replace(/^map-/, "")}-r`);
      const unreplaced = <T,>(all: Record<string, T>) =>
        Object.fromEntries(Object.entries(all).filter(([id]) => !onReplaced(id)));
      setDrafts(unreplaced);
      setDetailErrors(unreplaced);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const generateMissing = async () => {
    for (const p of planned.filter((p) => !p.generated)) {
      await generate({ only: p.id }, `Drawing ${p.label}…`);
    }
  };

  // Esc clears the selection first; only then does it leave the screen.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    if (moving) {
      event.preventDefault();
      setMoving(null);
      setMoveTarget(null);
    } else if (placing) {
      event.preventDefault();
      setPlacing(null);
    } else if (poiId) {
      event.preventDefault();
      setPoiId(null);
    } else if (confirmRegenerate) {
      event.preventDefault();
      setConfirmRegenerate(false);
    } else if (selectedId) {
      event.preventDefault();
      select(null);
    }
  };

  const onToolbarKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const buttons = [...(toolbarRef.current?.querySelectorAll("button:not(:disabled)") ?? [])];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === "ArrowRight" ? index + 1 : event.key === "ArrowLeft" ? index - 1 : null;
    if (next !== null && index >= 0 && next >= 0 && next < buttons.length) {
      event.preventDefault();
      (buttons[next] as HTMLButtonElement).focus();
    }
  };

  if (error && !view) {
    return (
      <p className="alert alert-inline" role="alert">
        {error}
      </p>
    );
  }
  if (!view) return <p className="muted">Loading map…</p>;

  const perDistrict = view.layout === "per_district";
  const missing = planned.filter((p) => !p.generated);
  const limits = activeMap ? zoomLimits(activeMap.geometry.bounds, size) : { min: 1, max: 1 };
  const home = activeMap ? homeView(activeMap, size) : null;
  const zoomPercent = viewport && home ? Math.round((viewport.k / home.k) * 100) : 100;
  const zoomBy = (factor: number) =>
    viewport && updateViewport(zoomAt(viewport, factor, size.width / 2, size.height / 2, limits));
  const scale = viewport ? niceLength(120 / viewport.k) : 0;

  return (
    <div className="map-explorer" onKeyDown={onKeyDown}>
      {perDistrict && planned.length > 0 && (
        <DistrictTabs
          planned={planned}
          activeId={activeId}
          panelId={tabPanelId}
          onChange={setActiveId}
        />
      )}

      {perDistrict && activePlan && (
        <DistrictBar
          planned={planned}
          active={activePlan}
          busy={locationBusy || busy !== null}
          onSearch={setActiveId}
          onRename={renameDistrict}
          onDeleteMap={() => void deleteMap(activePlan.id)}
          onDirectory={openDirectory}
        />
      )}
      {error && (
        <p className="alert alert-inline" role="alert">
          {error}
        </p>
      )}
      {placing && (
        <div className="map-notice map-notice-move" role="status">
          <span>
            {placing.kind === "add"
              ? "Click the map where the point of interest goes. Esc cancels."
              : `Click the new position of ${placing.poi.label}. Esc cancels.`}
          </span>
          <button type="button" className="btn btn-small" onClick={() => setPlacing(null)}>
            Cancel
          </button>
        </div>
      )}
      {view.damaged && view.damaged.length > 0 && (
        <div className="map-notice map-notice-stale" role="alert">
          <span>
            Some saved map data could not be read and is not shown:{" "}
            {view.damaged
              .map((d) =>
                d.kind === "maps"
                  ? "the maps themselves"
                  : `${{ building: "building", location: "Notable Location", poi: "point of interest" }[d.kind]} ${d.id}`,
              )
              .join(", ")}
            . Everything else is safe.
          </span>
          {!confirmDiscard && (
            <button type="button" className="btn btn-small" onClick={() => setConfirmDiscard(true)}>
              Discard damaged data…
            </button>
          )}
        </div>
      )}
      {confirmDiscard && (
        <ConfirmBar
          message="Delete the saved map data that could not be read? The settlement and all readable map data are kept."
          confirmLabel="Discard"
          onConfirm={() => {
            setConfirmDiscard(false);
            void changeLocations(() => maps.discardDamaged(settlementId)).then(
              (failure) => failure && setError(failure),
            );
          }}
          onCancel={() => setConfirmDiscard(false)}
        />
      )}
      {view.problem && (
        <p className="map-notice" role="note">
          {view.problem}
        </p>
      )}
      {activePlan?.stale && activeMap && (
        <div className="map-notice map-notice-stale" role="note">
          <span>The settlement has changed since this map was drawn. It is shown as it was.</span>
          {!confirmRegenerate && (
            <button
              type="button"
              className="btn btn-small"
              onClick={() => setConfirmRegenerate(true)}
            >
              Redraw map
            </button>
          )}
        </div>
      )}
      {moving && (
        <div className="map-notice map-notice-move" role="status">
          <span>
            Choose the building to move <strong>{moving.name}</strong> to
            {moving.district ? ` (in ${moving.district.label})` : ""}. Esc cancels.
          </span>
          <button
            type="button"
            className="btn btn-small"
            onClick={() => {
              setMoving(null);
              setMoveTarget(null);
            }}
          >
            Cancel move
          </button>
        </div>
      )}
      {moving && moveTarget && (
        <ConfirmBar
          message={`Move ${moving.name} to ${buildingLabel(moveTarget)}? Its marker number stays ${
            moving.marker
          }.${
            locationOn(moveTarget.id)
              ? ` ${locationOn(moveTarget.id)!.name} will be taken off that building and kept.`
              : ""
          }`}
          confirmLabel="Move"
          onConfirm={() => void confirmMove()}
          onCancel={() => setMoveTarget(null)}
        />
      )}
      {confirmRegenerate && activeMap && (
        <ConfirmBar
          message={`Redraw the map of ${activeMap.label}? Its buildings will be replaced.`}
          confirmLabel="Redraw"
          onConfirm={() => void generate({ only: activeMap.id }, `Redrawing ${activeMap.label}…`)}
          onCancel={() => setConfirmRegenerate(false)}
        />
      )}

      <div
        className={`map-workspace ${sideOpen && activeMap ? "" : "is-side-closed"}`}
        id={perDistrict ? tabPanelId : undefined}
        role={perDistrict ? "tabpanel" : undefined}
        aria-label={activePlan ? activePlan.label : undefined}
      >
        <div className="map-stage-column">
          {activeMap && viewport && (
            <div
              ref={toolbarRef}
              className="map-toolbar"
              role="toolbar"
              aria-label="Map controls"
              onKeyDown={onToolbarKey}
            >
              <button
                type="button"
                className="btn btn-small"
                aria-label="Zoom out"
                title="Zoom out (−)"
                disabled={viewport.k <= limits.min}
                onClick={() => zoomBy(1 / ZOOM_STEP)}
              >
                −
              </button>
              <span className="map-zoom" aria-live="polite" aria-label={`Zoom ${zoomPercent}%`}>
                {zoomPercent}%
              </span>
              <button
                type="button"
                className="btn btn-small"
                aria-label="Zoom in"
                title="Zoom in (+)"
                disabled={viewport.k >= limits.max}
                onClick={() => zoomBy(ZOOM_STEP)}
              >
                +
              </button>
              <button
                type="button"
                className="btn btn-small"
                title="Show the whole map (0)"
                onClick={() => command("fit")}
              >
                Fit map
              </button>
              <button
                type="button"
                className="btn btn-small"
                title="Back to the starting view (Home)"
                onClick={() => command("reset")}
              >
                Reset view
              </button>
              <span className="map-toolbar-gap" />
              {activeMap.scope.kind === "district" && (
                <button
                  type="button"
                  className="btn btn-small map-toggle"
                  aria-pressed={layers.district}
                  onClick={() => setLayers((l) => ({ ...l, district: !l.district }))}
                >
                  District area
                </button>
              )}
              <button
                type="button"
                className="btn btn-small map-toggle"
                aria-pressed={layers.markers}
                onClick={() => setLayers((l) => ({ ...l, markers: !l.markers }))}
              >
                Markers
              </button>
              <button
                type="button"
                className="btn btn-small map-toggle"
                aria-pressed={placing?.kind === "add"}
                title="Place a point of interest: then click the map"
                onClick={() => {
                  setMoving(null);
                  setPlacing((p) => (p?.kind === "add" ? null : { kind: "add" }));
                }}
              >
                Add point
              </button>
              <button
                type="button"
                className="btn btn-small"
                title="Save this map as a picture or as data"
                onClick={() => setExportOpen(true)}
              >
                Export…
              </button>
              <button
                type="button"
                className="btn btn-small map-toggle"
                aria-pressed={sideOpen}
                aria-label="Side panel"
                title={sideOpen ? "Hide the side panel" : "Show the side panel"}
                onClick={() => setSideOpen((o) => !o)}
              >
                {sideOpen ? "Hide panel" : "Show panel"}
              </button>
            </div>
          )}

          <div className="map-stage" ref={stageRef}>
            {activeMap && viewport ? (
              <>
                <MapCanvas
                  map={activeMap}
                  viewport={viewport}
                  size={size}
                  limits={limits}
                  onViewport={updateViewport}
                  selectedId={selectedId}
                  onSelect={pick}
                  onCommand={command}
                  showDistrict={layers.district}
                  showMarkers={layers.markers}
                  markers={legend?.markers ?? []}
                  describedBy={describedBy}
                  placing={placing !== null}
                  onPoint={(point) => void placeAt(point)}
                  selectedPoiId={selectedPoi?.id ?? null}
                  onSelectPoi={(id) => {
                    setPoiId(id);
                    if (activeId) setSelection((s) => ({ ...s, [activeId]: null }));
                    setPanels((p) => ({ ...p, inspector: true }));
                  }}
                />
                <div className="map-scale" aria-hidden="true">
                  <span style={{ width: scale * viewport.k }} />
                  {scale >= 1000 ? `${scale / 1000} km` : `${scale} m`}
                </div>
              </>
            ) : (
              <EmptyMap
                settlementName={settlementName}
                plan={activePlan}
                perDistrict={perDistrict}
                missing={missing.length}
                canGenerate={!view.problem}
                busy={busy !== null}
                onGenerate={() =>
                  void (perDistrict && activePlan
                    ? generate({ only: activePlan.id }, `Drawing ${activePlan.label}…`)
                    : generate({}, `Drawing the map of ${settlementName}…`))
                }
                onGenerateMissing={() => void generateMissing()}
              />
            )}
            {busy && (
              <div className="map-busy" role="status">
                {busy}
              </div>
            )}
          </div>
        </div>

        {activeMap && sideOpen && (
          <aside className="map-side" aria-label="Map details">
            <Panel
              title="Location Inspector"
              open={panels.inspector}
              onToggle={() => setPanels((p) => ({ ...p, inspector: !p.inspector }))}
            >
              {newPoiAt && newPoiAt.mapId === activeMap.id ? (
                <PoiForm
                  title="New point of interest"
                  busy={locationBusy}
                  submitLabel="Add point"
                  onSave={(label, notes) => void addPoi(label, notes)}
                  onCancel={() => setNewPoiAt(null)}
                />
              ) : selectedPoi ? (
                <PoiInspector
                  poi={selectedPoi}
                  busy={locationBusy}
                  onSave={async (label, notes) => {
                    const failure = await changeLocations(() =>
                      maps.updatePoi(settlementId, selectedPoi.id, label, notes),
                    );
                    if (failure) setError(failure);
                    return !failure;
                  }}
                  onMove={() => setPlacing({ kind: "move", poi: selectedPoi })}
                  onDelete={() =>
                    void changeLocations(() => maps.deletePoi(settlementId, selectedPoi.id)).then(
                      (failure) => (failure ? setError(failure) : setPoiId(null)),
                    )
                  }
                  onLocate={() =>
                    viewport && updateViewport(centreOn(viewport, selectedPoi.position, size))
                  }
                  onClear={() => setPoiId(null)}
                />
              ) : (
                <LocationInspector
                  map={activeMap}
                  building={selected}
                  details={selectedDetails}
                  loading={
                    selectedId && !selectedDetails && !detailErrors[selectedId]
                      ? summaries.has(selectedId)
                        ? "Loading details…"
                        : "Generating details…"
                      : null
                  }
                  error={selectedId ? (detailErrors[selectedId] ?? null) : null}
                  draft={selectedId ? drafts[selectedId] : undefined}
                  onDraft={(draft) =>
                    selectedId &&
                    setDrafts(({ [selectedId]: _, ...rest }) =>
                      draft ? { ...rest, [selectedId]: draft } : rest,
                    )
                  }
                  onSave={(edits: BuildingEdits, version: number) =>
                    changeDetails((d) =>
                      maps.editBuilding(settlementId, d.map_id, d.building_id, version, edits),
                    )
                  }
                  onRegenerate={(discard: boolean) =>
                    changeDetails((d) =>
                      maps.regenerateBuilding(
                        settlementId,
                        d.map_id,
                        d.building_id,
                        d.version,
                        discard,
                      ),
                    )
                  }
                  onReset={() =>
                    changeDetails((d) =>
                      maps.resetBuilding(settlementId, d.map_id, d.building_id, d.version),
                    )
                  }
                  onRetry={() => {
                    if (selectedId) setDetailErrors(({ [selectedId]: _, ...rest }) => rest);
                    setRetry((r) => r + 1);
                  }}
                  onLocate={() =>
                    selected &&
                    viewport &&
                    updateViewport(centreOn(viewport, selected.centroid, size))
                  }
                  onClear={() => select(null)}
                  location={selectedLocation}
                  onMakeNotable={() => {
                    setDialogError(null);
                    setDialogOpen(true);
                  }}
                  onEditLocation={async (edit: LocationEdit) => {
                    if (!selectedLocation) return false;
                    const failure = await changeLocations(() =>
                      maps.updateLocation(
                        settlementId,
                        selectedLocation.id,
                        selectedLocation.version,
                        edit,
                      ),
                    );
                    if (failure) setError(failure);
                    return !failure;
                  }}
                  onUnassign={async () => {
                    if (!selectedLocation) return false;
                    const failure = await changeLocations(() =>
                      maps.unassignLocation(settlementId, selectedLocation.id),
                    );
                    if (failure) setError(failure);
                    return !failure;
                  }}
                  onReassign={() => {
                    setMoving(selectedLocation ?? null);
                    setMoveTarget(null);
                  }}
                />
              )}
            </Panel>
            <Panel
              title="Legend"
              open={panels.legend}
              onToggle={() => setPanels((p) => ({ ...p, legend: !p.legend }))}
            >
              <Legend
                map={activeMap}
                entries={legend?.entries ?? []}
                onGo={(entry) => {
                  const target = entry.target;
                  if (target.kind === "notable_location") {
                    goTo(activeMap.id, target.building_id);
                  } else {
                    const poi = pois.find((p) => p.id === target.poi_id);
                    if (poi) goToPoi(poi);
                  }
                }}
              />
            </Panel>
            <Panel
              title="Directory"
              open={panels.locations}
              onToggle={() => setPanels((p) => ({ ...p, locations: !p.locations }))}
            >
              <LocationDirectory
                locations={locations}
                pois={pois}
                byDistrict={perDistrict}
                scope={scope}
                onScope={setScope}
                currentDistrict={
                  activeMap.scope.kind === "district" ? activeMap.scope.district : undefined
                }
                onGo={(l) => goTo(l.placement.map_id, l.placement.building_id)}
                onGoPoi={goToPoi}
                searchRef={directorySearch}
              />
            </Panel>
            <Panel
              title="Building directory"
              open={panels.directory}
              onToggle={() => setPanels((p) => ({ ...p, directory: !p.directory }))}
            >
              <BuildingDirectory
                map={activeMap}
                summaries={summaries}
                selectedId={selectedId}
                onChoose={(b) => {
                  select(b);
                  if (viewport) updateViewport(centreOn(viewport, b.centroid, size));
                }}
              />
            </Panel>
          </aside>
        )}
      </div>

      {exportOpen && activeMap && (
        <ExportDialog
          mapId={activeMap.id}
          mapLabel={activeMap.label}
          districts={perDistrict}
          drawn={view.maps?.maps.length ?? 0}
          onExport={(request) => maps.export(settlementId, request)}
          onClose={() => setExportOpen(false)}
        />
      )}
      {dialogOpen && activeMap && selected && (
        <LocationDialog
          buildingLabel={buildingLabel(selected)}
          mapDistrict={activeMap.scope.kind === "district" ? activeMap.scope.district : undefined}
          mapId={activeMap.id}
          buildingId={selected.id}
          locations={locations}
          busy={locationBusy}
          error={dialogError}
          onConfirm={(choice) => void chooseLocation(choice)}
          onCancel={() => setDialogOpen(false)}
        />
      )}

      <p id={describedBy} className="visually-hidden">
        Drag to pan, scroll to zoom and click a building to select it. With the map focused, the
        arrow keys pan, plus and minus zoom, 0 shows the whole map, Home resets the view and Escape
        clears the selection. The building directory lists every building.
        {selected ? ` Selected: ${buildingLabel(selected)}.` : ""}
      </p>
    </div>
  );
}

/** District tabs: names and IDs come from the settlement's own districts. */
function DistrictTabs({
  planned,
  activeId,
  panelId,
  onChange,
}: {
  planned: PlannedMapView[];
  activeId: string | null;
  panelId: string;
  onChange: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = planned.findIndex((p) => p.id === activeId);
    let next: number | null = null;
    if (event.key === "ArrowRight") next = (index + 1) % planned.length;
    if (event.key === "ArrowLeft") next = (index - 1 + planned.length) % planned.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = planned.length - 1;
    if (next !== null) {
      event.preventDefault();
      onChange(planned[next].id);
      ref.current?.querySelectorAll<HTMLButtonElement>("[role=tab]")[next]?.focus();
    }
  };
  return (
    <div
      ref={ref}
      className="map-districts"
      role="tablist"
      aria-label="Districts"
      onKeyDown={onKey}
    >
      {planned.map((p) => {
        const active = p.id === activeId;
        return (
          <button
            key={p.id}
            type="button"
            role="tab"
            aria-selected={active}
            aria-controls={panelId}
            tabIndex={active ? 0 : -1}
            className={`map-district-tab ${active ? "is-active" : ""}`}
            onClick={() => onChange(p.id)}
          >
            {p.label}
            {!p.generated && <span className="map-badge">No map</span>}
            {p.stale && <span className="map-badge map-badge-stale">Changed</span>}
          </button>
        );
      })}
    </div>
  );
}

function EmptyMap({
  settlementName,
  plan,
  perDistrict,
  missing,
  canGenerate,
  busy,
  onGenerate,
  onGenerateMissing,
}: {
  settlementName: string;
  plan: PlannedMapView | null;
  perDistrict: boolean;
  missing: number;
  canGenerate: boolean;
  busy: boolean;
  onGenerate: () => void;
  onGenerateMissing: () => void;
}) {
  const subject = perDistrict && plan ? plan.label : settlementName;
  return (
    <div className="map-empty">
      <svg className="map-empty-art" viewBox="-50 -50 100 100" aria-hidden="true">
        <circle r="44" />
        <circle r="30" />
        <path d="M0 -46L6 -6L46 0L6 6L0 46L-6 6L-46 0L-6 -6Z" />
      </svg>
      <h3 className="display map-empty-title">
        {perDistrict && plan ? `${plan.label} has no map yet` : "No map yet"}
      </h3>
      <p className="muted">
        {canGenerate
          ? perDistrict
            ? `Each district of ${settlementName} has its own map. Drawing one leaves the others as they are, and nothing about the settlement changes.`
            : `Draw a map of ${settlementName} from its saved details. Nothing about the settlement changes.`
          : "This settlement cannot be mapped yet."}
      </p>
      {canGenerate && (
        <div className="action-bar map-empty-actions">
          <button type="button" className="btn btn-primary" disabled={busy} onClick={onGenerate}>
            {perDistrict ? `Generate ${subject} map` : "Generate map"}
          </button>
          {perDistrict && missing > 1 && (
            <button type="button" className="btn" disabled={busy} onClick={onGenerateMissing}>
              Generate all {missing} missing districts
            </button>
          )}
        </div>
      )}
    </div>
  );
}
