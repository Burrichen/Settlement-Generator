import {
  memo,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { layersFor, ringPath, type MapLayers } from "./paths";
import { indexFor } from "./spatial";
import type { MapBuilding, MapMarker, Point, SettlementMap } from "./types";
import {
  PAN_STEP,
  ZOOM_STEP,
  pan,
  toMap,
  toScreen,
  zoomAt,
  type Size,
  type Viewport,
} from "./viewport";

export type CanvasCommand = "fit" | "reset";

interface Props {
  map: SettlementMap;
  viewport: Viewport;
  size: Size;
  limits: { min: number; max: number };
  onViewport: (v: Viewport) => void;
  selectedId: string | null;
  onSelect: (building: MapBuilding | null) => void;
  onCommand: (command: CanvasCommand) => void;
  showDistrict: boolean;
  showMarkers: boolean;
  markers: MapMarker[];
  describedBy: string;
  /** Placing a point: a click reports the map position instead of selecting. */
  placing?: boolean;
  onPoint?: (p: Point) => void;
  /** The selected point of interest, and selecting one by its pin. */
  selectedPoiId?: string | null;
  onSelectPoi?: (id: string) => void;
}

/** Pointer movement (px) below which a press counts as a click, not a drag. */
const CLICK_SLOP = 4;

/**
 * Marker pins, in screen pixels: the tip touches the building's centre and
 * the numbered head stands above it, so a marker never hides its building
 * and stays the same size at every zoom.
 */
const PIN_HEAD_Y = -19;
const PIN_RADIUS = 10;

/**
 * The map: static layers drawn once per map as a few merged paths, under a
 * single transform. Pan and zoom change only that transform; hover and
 * selection only redraw one outline. Buildings are picked from their
 * footprints through a spatial index, not from the drawing.
 */
export function MapCanvas({
  map,
  viewport,
  size,
  limits,
  onViewport,
  selectedId,
  onSelect,
  onCommand,
  showDistrict,
  showMarkers,
  markers,
  describedBy,
  placing = false,
  onPoint,
  selectedPoiId = null,
  onSelectPoi,
}: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoverId, setHoverId] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const layers = layersFor(map.geometry);
  const index = indexFor(map.geometry.buildings);
  const byId = useMemo(
    () => new Map(map.geometry.buildings.map((b) => [b.id, b])),
    [map.geometry.buildings],
  );

  // The latest values, for native listeners registered once.
  const latest = useRef({ viewport, limits, onViewport });
  latest.current = { viewport, limits, onViewport };

  const local = (clientX: number, clientY: number): Point => {
    const rect = svgRef.current?.getBoundingClientRect();
    return [clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0)];
  };

  // Wheel zoom needs a non-passive listener to stop the page scrolling.
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const { viewport: v, limits: l, onViewport: set } = latest.current;
      const rect = svg.getBoundingClientRect();
      const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      set(zoomAt(v, factor, event.clientX - rect.left, event.clientY - rect.top, l));
    };
    svg.addEventListener("wheel", onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", onWheel);
  }, []);

  const hitAt = (clientX: number, clientY: number) => {
    const [px, py] = local(clientX, clientY);
    return index.hit(toMap(latest.current.viewport, px, py));
  };

  const onMouseDown = (event: ReactMouseEvent<SVGSVGElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    svgRef.current?.focus({ preventScroll: true });
    // A press on a marker pin selects the building it stands on.
    const pinned = (event.target as Element).closest?.("[data-marker-building]");
    const pinnedId = pinned?.getAttribute("data-marker-building") ?? null;
    const poiPin = (event.target as Element).closest?.("[data-marker-poi]");
    const poiId = poiPin?.getAttribute("data-marker-poi") ?? null;
    const start = { x: event.clientX, y: event.clientY };
    let last = start;
    let moved = false;
    const move = (e: MouseEvent) => {
      if (!moved && Math.hypot(e.clientX - start.x, e.clientY - start.y) > CLICK_SLOP) {
        moved = true;
        setDragging(true);
      }
      if (moved) {
        const { viewport: v, onViewport: set } = latest.current;
        set(pan(v, e.clientX - last.x, e.clientY - last.y));
        last = { x: e.clientX, y: e.clientY };
      }
    };
    const up = (e: MouseEvent) => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setDragging(false);
      if (moved) return;
      if (placing && onPoint) {
        const [px, py] = local(e.clientX, e.clientY);
        onPoint(toMap(latest.current.viewport, px, py));
      } else if (poiId && onSelectPoi) {
        onSelectPoi(poiId);
      } else {
        const pin = pinnedId ? byId.get(pinnedId) : undefined;
        onSelect(pin ?? hitAt(e.clientX, e.clientY));
      }
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const onMouseMove = (event: ReactMouseEvent<SVGSVGElement>) => {
    if (dragging) return;
    const id = hitAt(event.clientX, event.clientY)?.id ?? null;
    if (id !== hoverId) setHoverId(id);
  };

  const onKeyDown = (event: KeyboardEvent<SVGSVGElement>) => {
    const centre = { x: size.width / 2, y: size.height / 2 };
    const step: Record<string, [number, number]> = {
      ArrowLeft: [PAN_STEP, 0],
      ArrowRight: [-PAN_STEP, 0],
      ArrowUp: [0, PAN_STEP],
      ArrowDown: [0, -PAN_STEP],
    };
    if (step[event.key]) {
      event.preventDefault();
      onViewport(pan(viewport, ...step[event.key]));
    } else if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      onViewport(zoomAt(viewport, ZOOM_STEP, centre.x, centre.y, limits));
    } else if (event.key === "-" || event.key === "_") {
      event.preventDefault();
      onViewport(zoomAt(viewport, 1 / ZOOM_STEP, centre.x, centre.y, limits));
    } else if (event.key === "0") {
      event.preventDefault();
      onCommand("fit");
    } else if (event.key === "Home") {
      event.preventDefault();
      onCommand("reset");
    }
  };

  const hovered = hoverId && hoverId !== selectedId ? byId.get(hoverId) : undefined;
  const selected = selectedId ? byId.get(selectedId) : undefined;

  return (
    <svg
      ref={svgRef}
      className={`map-canvas ground-${groundGroup(map.geometry.terrain.ground)} ${
        dragging ? "is-dragging" : placing ? "is-placing" : hovered ? "is-pointing" : ""
      }`}
      width={size.width}
      height={size.height}
      viewBox={`0 0 ${size.width} ${size.height}`}
      role="application"
      aria-roledescription="map"
      aria-label={`Map of ${map.label}`}
      aria-describedby={describedBy}
      tabIndex={0}
      data-zoom={viewport.k}
      data-tx={viewport.tx}
      data-ty={viewport.ty}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseLeave={() => setHoverId(null)}
      onKeyDown={onKeyDown}
    >
      <rect className="map-ground" width={size.width} height={size.height} />
      <g
        data-testid="map-transform"
        transform={`translate(${viewport.tx} ${viewport.ty}) scale(${viewport.k})`}
      >
        <StaticLayers layers={layers} />
        {showDistrict && map.scope.kind === "district" && (
          <path className="map-district-outline" d={layers.outline} />
        )}
        {hovered && <path className="map-hover" d={ringPath(hovered.footprint)} />}
        {selected && (
          <path
            className="map-selection"
            data-testid="map-selection"
            d={ringPath(selected.footprint)}
          />
        )}
      </g>
      {showMarkers && (
        <g className="map-markers" aria-hidden="true">
          {markers.map((m) => {
            const [x, y] = toScreen(viewport, m.position);
            const notable = m.target.kind === "notable_location";
            const building =
              m.target.kind === "notable_location" ? m.target.building_id : undefined;
            const poi = m.target.kind === "point_of_interest" ? m.target.poi_id : undefined;
            const chosen = building ? building === selectedId : poi === selectedPoiId;
            return (
              <g
                key={`${m.target.kind}-${m.number}`}
                className={`map-marker map-marker-${notable ? "location" : "poi"} ${
                  chosen ? "is-selected" : ""
                }`}
                data-marker={m.number}
                data-marker-building={building}
                data-marker-poi={poi}
                transform={`translate(${x} ${y})`}
              >
                <line
                  className="map-marker-stem"
                  x1={0}
                  y1={0}
                  x2={0}
                  y2={PIN_HEAD_Y + PIN_RADIUS}
                />
                <circle className="map-marker-tip" r={2} />
                {notable ? (
                  <circle className="map-marker-head" cy={PIN_HEAD_Y} r={PIN_RADIUS} />
                ) : (
                  <rect
                    className="map-marker-head"
                    x={-PIN_RADIUS * 0.8}
                    y={PIN_HEAD_Y - PIN_RADIUS * 0.8}
                    width={PIN_RADIUS * 1.6}
                    height={PIN_RADIUS * 1.6}
                    transform={`rotate(45 0 ${PIN_HEAD_Y})`}
                  />
                )}
                <text y={PIN_HEAD_Y} dy="0.35em">
                  {m.number}
                </text>
              </g>
            );
          })}
        </g>
      )}
    </svg>
  );
}

/** Palette group for the ground (see the `--map-ground-*` tokens). */
function groundGroup(ground: string): string {
  if (["sand", "rock", "wasteland", "hills"].includes(ground)) return "arid";
  if (["snow", "ice"].includes(ground)) return "cold";
  if (["marsh", "jungle"].includes(ground)) return "wet";
  if (["crystal", "ichor", "cavern"].includes(ground)) return "strange";
  return "green";
}

const circles = (points: { centre: Point; radius: number }[]) =>
  points
    .map(
      ({ centre: [x, y], radius: r }) =>
        `M${x - r} ${y}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`,
    )
    .join("");

/** Everything that never changes for a map: drawn once per map. */
const StaticLayers = memo(function StaticLayers({ layers }: { layers: MapLayers }) {
  const dots = (points: Point[], radius: number) =>
    circles(points.map((centre) => ({ centre, radius })));
  return (
    <g className="map-static">
      <path className="map-fields" d={layers.fields} />
      <path className="map-water" d={layers.water} />
      <path className="map-trees" d={circles(layers.trees)} />
      <path className="map-square" d={layers.squares} />
      <path
        className="map-street map-road"
        d={layers.streets.road}
        strokeWidth={layers.streetWidths.road}
      />
      <path
        className="map-street map-street-street"
        d={layers.streets.street}
        strokeWidth={layers.streetWidths.street}
      />
      <path
        className="map-street map-lane"
        d={layers.streets.lane}
        strokeWidth={layers.streetWidths.lane}
      />
      <path className="map-bridges" d={dots(layers.bridges, 3)} />
      <path className="map-buildings" data-testid="map-buildings" d={layers.buildings} />
      <path className="map-walls" d={layers.walls} />
      <path className="map-towers" d={dots(layers.towers, 2.5)} />
      <path className="map-gates" d={dots(layers.gates, 3)} />
    </g>
  );
});
