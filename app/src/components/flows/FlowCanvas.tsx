// The node graph: steps as nodes on a pannable, zoomable canvas. Drag a node
// onto a + to connect it there (or onto empty space to disconnect it); click
// a + to add a step. Every change is an edit the core applies.
import {
  forwardRef,
  useImperativeHandle,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type WheelEvent,
} from "react";
import type { FlowPosition, Named, NodeText, StepKind } from "../../api";
import { NODE_H, NODE_W, type FlowLayout, type LaidPort } from "./layout";

export interface CanvasHandle {
  fit: () => void;
  zoomBy: (factor: number) => void;
  actualSize: () => void;
  centerOn: (id: string) => void;
}

interface View {
  x: number;
  y: number;
  zoom: number;
}

const MIN_ZOOM = 0.2;
const MAX_ZOOM = 2;
/** How close (in canvas units) a drop must be to a + to connect there. */
const SNAP = 30;

const clamp = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

export type DropTarget = { port: FlowPosition } | { at: [number, number] };

export const FlowCanvas = forwardRef<
  CanvasHandle,
  {
    layout: FlowLayout;
    texts: Map<string, NodeText>;
    problems: Map<string, "error" | "warning">;
    selected: string | null;
    matches: Set<string>;
    kinds: Named[];
    onSelect: (id: string | null) => void;
    onAdd: (at: FlowPosition, kind: StepKind) => void;
    onDrop: (id: string, target: DropTarget) => void;
  }
>(function FlowCanvas(
  { layout, texts, problems, selected, matches, kinds, onSelect, onAdd, onDrop },
  ref,
) {
  const viewport = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<View>({ x: 0, y: 0, zoom: 1 });
  const [palette, setPalette] = useState<string | null>(null);
  const pan = useRef<{ x: number; y: number; from: View } | null>(null);
  const [drag, setDrag] = useState<{
    id: string;
    startX: number;
    startY: number;
    dx: number;
    dy: number;
    moved: boolean;
  } | null>(null);

  const size = () => {
    const el = viewport.current;
    return { w: el?.clientWidth ?? 0, h: el?.clientHeight ?? 0 };
  };

  useImperativeHandle(
    ref,
    () => ({
      fit: () => {
        const { w, h } = size();
        if (w === 0 || h === 0) return;
        const zoom = clamp(Math.min(w / layout.width, h / layout.height, 1));
        setView({
          zoom,
          x: (w - layout.width * zoom) / 2,
          y: Math.max(0, (h - layout.height * zoom) / 2),
        });
      },
      zoomBy: (factor) => {
        const { w, h } = size();
        setView((v) => zoomAround(v, clamp(v.zoom * factor), w / 2, h / 2));
      },
      actualSize: () => setView((v) => ({ ...v, zoom: 1 })),
      centerOn: (id) => {
        const node = layout.nodes.find((n) => n.id === id);
        const { w, h } = size();
        if (!node) return;
        setView((v) => ({
          ...v,
          x: w / 2 - (node.x + NODE_W / 2) * v.zoom,
          y: h / 2 - (node.y + NODE_H / 2) * v.zoom,
        }));
      },
    }),
    [layout],
  );

  /** A client point in canvas units. */
  const toCanvas = (clientX: number, clientY: number): [number, number] => {
    const rect = viewport.current?.getBoundingClientRect();
    return [
      (clientX - (rect?.left ?? 0) - view.x) / view.zoom,
      (clientY - (rect?.top ?? 0) - view.y) / view.zoom,
    ];
  };

  const nearestPort = (x: number, y: number, exclude: string): LaidPort | null => {
    let best: LaidPort | null = null;
    let bestDistance = SNAP;
    for (const port of layout.ports) {
      // A step cannot be dropped inside itself.
      if ("step" in port.at && port.at.step === exclude) continue;
      const d = Math.hypot(port.x - x, port.y - y);
      if (d < bestDistance) {
        best = port;
        bestDistance = d;
      }
    }
    return best;
  };

  const onBackgroundDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget && !(event.target instanceof SVGElement)) return;
    setPalette(null);
    onSelect(null);
    pan.current = { x: event.clientX, y: event.clientY, from: view };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onBackgroundMove = (event: PointerEvent<HTMLDivElement>) => {
    const p = pan.current;
    if (!p) return;
    setView({ ...p.from, x: p.from.x + event.clientX - p.x, y: p.from.y + event.clientY - p.y });
  };
  const onBackgroundUp = () => {
    pan.current = null;
  };

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    const rect = viewport.current?.getBoundingClientRect();
    const mx = event.clientX - (rect?.left ?? 0);
    const my = event.clientY - (rect?.top ?? 0);
    const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
    setView((v) => zoomAround(v, clamp(v.zoom * factor), mx, my));
  };

  const onNodeDown = (id: string, event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    setPalette(null);
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDrag({ id, startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, moved: false });
  };
  const onNodeMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag) return;
    const dx = (event.clientX - drag.startX) / view.zoom;
    const dy = (event.clientY - drag.startY) / view.zoom;
    setDrag({ ...drag, dx, dy, moved: drag.moved || Math.hypot(dx, dy) > 5 });
  };
  const onNodeUp = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag) return;
    const current = drag;
    setDrag(null);
    if (!current.moved) {
      onSelect(current.id);
      return;
    }
    const node = layout.nodes.find((n) => n.id === current.id);
    if (!node) return;
    const [px, py] = toCanvas(event.clientX, event.clientY);
    const port = nearestPort(px, py, current.id);
    if (port) onDrop(current.id, { port: port.at });
    else
      onDrop(current.id, {
        at: [Math.round(node.x + current.dx), Math.round(node.y + current.dy)],
      });
  };

  const onNodeKey = (id: string, event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(id);
    }
  };

  const dropPort =
    drag?.moved && layout.nodes.find((n) => n.id === drag.id)
      ? (() => {
          const node = layout.nodes.find((n) => n.id === drag.id)!;
          return nearestPort(node.x + drag.dx + 22, node.y + drag.dy, drag.id)?.key ?? null;
        })()
      : null;

  return (
    <div
      ref={viewport}
      className={`flow-canvas ${drag?.moved ? "is-dragging" : ""}`}
      aria-label="Flow diagram"
      role="application"
      onPointerDown={onBackgroundDown}
      onPointerMove={onBackgroundMove}
      onPointerUp={onBackgroundUp}
      onWheel={onWheel}
    >
      <div
        className="flow-stage"
        style={{
          width: layout.width,
          height: layout.height,
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})`,
        }}
      >
        <svg className="flow-edges" width={layout.width} height={layout.height} aria-hidden="true">
          <defs>
            <marker
              id="flow-arrow"
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="7"
              markerHeight="7"
              orient="auto-start-reverse"
            >
              <path d="M 0 0 L 10 5 L 0 10 z" className="flow-arrow" />
            </marker>
          </defs>
          {layout.edges.map((edge) => (
            <g key={edge.id}>
              <path
                d={edge.path}
                className={`flow-edge ${edge.open ? "is-open" : ""}`}
                markerEnd={edge.open ? undefined : "url(#flow-arrow)"}
              />
              {edge.label && edge.labelAt && (
                <text x={edge.labelAt[0]} y={edge.labelAt[1]} className="flow-edge-label">
                  <title>{edge.label}</title>
                  {edge.label.length > 46 ? `${edge.label.slice(0, 45)}…` : edge.label}
                </text>
              )}
            </g>
          ))}
        </svg>

        {layout.ports.map((port) => (
          <div
            key={port.key}
            className={`flow-port ${dropPort === port.key ? "is-target" : ""}`}
            style={{ left: port.x - 11, top: port.y - 11 }}
          >
            <button
              type="button"
              className="flow-port-button"
              aria-label={port.hint ? `${port.hint} here` : "Add a step here"}
              aria-expanded={palette === port.key}
              onPointerDown={(e) => e.stopPropagation()}
              onClick={() => setPalette(palette === port.key ? null : port.key)}
            >
              +
            </button>
            {port.hint && <span className="flow-port-hint">{port.hint}</span>}
            {palette === port.key && (
              <div
                className="flow-palette"
                role="menu"
                aria-label="Step kinds"
                onPointerDown={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.key === "Escape" && setPalette(null)}
              >
                {kinds.map((k) => (
                  <button
                    key={k.id}
                    type="button"
                    role="menuitem"
                    className="flow-palette-item"
                    onClick={() => {
                      setPalette(null);
                      onAdd(port.at, k.id as StepKind);
                    }}
                  >
                    {k.name}
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}

        {layout.nodes.map((node) => {
          const text = texts.get(node.id);
          const problem = problems.get(node.id);
          const dragging = drag?.id === node.id && drag.moved;
          const off = node.step.active === false || node.step.status === "planned";
          return (
            <div
              key={node.id}
              role="button"
              tabIndex={0}
              aria-pressed={selected === node.id}
              aria-label={`${text?.kind ?? node.step.kind}: ${text?.text ?? node.id}`}
              className={[
                "flow-node",
                `flow-node-${node.step.kind}`,
                selected === node.id ? "is-selected" : "",
                matches.has(node.id) ? "is-match" : "",
                node.detached ? "is-detached" : "",
                off ? "is-off" : "",
                problem ? `has-${problem}` : "",
                dragging ? "is-dragging" : "",
              ].join(" ")}
              style={{
                left: node.x,
                top: node.y,
                width: NODE_W,
                height: NODE_H,
                transform: dragging ? `translate(${drag.dx}px, ${drag.dy}px)` : undefined,
              }}
              onPointerDown={(e) => onNodeDown(node.id, e)}
              onPointerMove={onNodeMove}
              onPointerUp={onNodeUp}
              onKeyDown={(e) => onNodeKey(node.id, e)}
            >
              <span className="flow-node-kind">
                {text?.kind ?? node.step.kind}
                {problem && (
                  <span className="flow-node-flag" aria-hidden="true">
                    {problem === "error" ? " ●" : " ○"}
                  </span>
                )}
              </span>
              <span className="flow-node-text">{text?.text ?? node.id}</span>
              {text?.when && <span className="flow-node-when">Only when {text.when}</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
});

function zoomAround(v: View, zoom: number, mx: number, my: number): View {
  return {
    zoom,
    x: mx - ((mx - v.x) * zoom) / v.zoom,
    y: my - ((my - v.y) * zoom) / v.zoom,
  };
}
