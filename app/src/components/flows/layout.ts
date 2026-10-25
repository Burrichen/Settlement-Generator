// Lays a flow out as a node graph. A flow is a tree of step lists (the
// flow's own steps, a repeat's body, each case of a condition…); each list
// runs down a vertical rail, and a container's lists hang off to its right.
// Ports are the gaps between steps where a step can be added or dropped.
import type { Flow, FlowPosition, FlowStep, Slot } from "../../api";

export const NODE_W = 240;
export const NODE_H = 76;
/** Height of the gap (with its port) before and after each step. */
const GAP = 40;
/** How far a container's lists sit to the right of its rail. */
const INDENT = 56;
const COLUMN_GAP = 36;
/** The rail runs this far in from a node's left edge. */
export const RAIL = 22;
const MARGIN = 40;

export interface LaidNode {
  id: string;
  step: FlowStep;
  x: number;
  y: number;
  /** Taken out of the flow: never runs. */
  detached: boolean;
}

export interface LaidEdge {
  id: string;
  /** SVG path data. */
  path: string;
  label?: string;
  labelAt?: [number, number];
  /** Leads to an empty place rather than a step. */
  open?: boolean;
}

export interface LaidPort {
  key: string;
  at: FlowPosition;
  x: number;
  y: number;
  /** Shown on an empty list, e.g. "Add the first step". */
  hint?: string;
}

export interface FlowLayout {
  nodes: LaidNode[];
  edges: LaidEdge[];
  ports: LaidPort[];
  width: number;
  height: number;
}

/** The lists inside a step, each with the label its edge carries. */
export function listsOf(
  step: FlowStep,
  caseText: (index: number) => string = (i) => `Case ${i + 1}`,
): { slot: Slot; steps: FlowStep[]; label: string }[] {
  switch (step.kind) {
    case "repeat":
      return [{ slot: { list: "body", step: step.id }, steps: step.steps, label: "Each time" }];
    case "for_each":
      return [
        {
          slot: { list: "body", step: step.id },
          steps: step.steps ?? [],
          label: step.pairs ? "For each pair" : "For each",
        },
      ];
    case "create_object":
      return [
        { slot: { list: "body", step: step.id }, steps: step.steps ?? [], label: "Inside it" },
      ];
    case "branch":
      return [
        ...step.cases.map((c, i) => ({
          slot: { list: "case", step: step.id, case: i } as Slot,
          steps: c.steps,
          label: `${i === 0 ? "IF" : "OTHERWISE IF"} ${caseText(i)}`,
        })),
        {
          slot: { list: "otherwise", step: step.id },
          steps: step.otherwise ?? [],
          label: "OTHERWISE",
        },
      ];
    default:
      return [];
  }
}

export function slotKey(slot: Slot): string {
  switch (slot.list) {
    case "top":
      return "top";
    case "body":
      return `body:${slot.step}`;
    case "case":
      return `case:${slot.step}:${slot.case}`;
    case "otherwise":
      return `otherwise:${slot.step}`;
  }
}

export function layoutFlow(
  flow: Flow,
  caseText: (step: string, index: number) => string = (_, i) => `Case ${i + 1}`,
): FlowLayout {
  const nodes: LaidNode[] = [];
  const edges: LaidEdge[] = [];
  const ports: LaidPort[] = [];

  /** Lays out `steps` with its rail at `x + RAIL`, from `y`. */
  const list = (
    steps: FlowStep[],
    slot: Slot | null,
    x: number,
    y: number,
    detached: boolean,
  ): { width: number; height: number } => {
    const rail = x + RAIL;
    let cy = y;
    let width = NODE_W;
    let previousBottom: number | null = null;
    const key = slot ? slotKey(slot) : "";
    const port = (index: number, py: number, hint?: string) => {
      if (slot)
        ports.push({ key: `${key}:${index}`, at: { ...slot, index }, x: rail, y: py, hint });
    };
    if (steps.length === 0) {
      port(0, cy + GAP / 2, "Add a step");
      return { width, height: GAP };
    }
    steps.forEach((step, index) => {
      port(index, cy + GAP / 2);
      const top = cy + GAP;
      if (previousBottom !== null) {
        edges.push({ id: `next:${step.id}`, path: `M ${rail} ${previousBottom} V ${top}` });
      }
      nodes.push({ id: step.id, step, x, y: top, detached });
      cy = top + NODE_H;
      previousBottom = cy;
      const lists = listsOf(step, (i) => caseText(step.id, i));
      if (lists.length > 0) {
        let cx = x + INDENT;
        let tallest = 0;
        const from = x + INDENT + RAIL;
        for (const sub of lists) {
          const labelY = cy + 18;
          const startY = cy + 28;
          const laid = list(sub.steps, detached ? null : sub.slot, cx, startY, detached);
          const subRail = cx + RAIL;
          edges.push({
            id: `into:${slotKey(sub.slot)}`,
            path:
              subRail === from
                ? `M ${from} ${cy} V ${startY + GAP / 2}`
                : `M ${from} ${cy} V ${labelY - 8} H ${subRail} V ${startY + GAP / 2}`,
            label: sub.label,
            labelAt: [subRail + 10, labelY],
            open: sub.steps.length === 0,
          });
          cx += laid.width + COLUMN_GAP;
          tallest = Math.max(tallest, laid.height + 28);
        }
        width = Math.max(width, cx - COLUMN_GAP - x);
        cy += tallest;
      }
    });
    // The place after the last step.
    if (slot) {
      port(steps.length, cy + GAP / 2);
      edges.push({
        id: `end:${key}`,
        path: `M ${rail} ${previousBottom} V ${cy + GAP / 2}`,
        open: true,
      });
    }
    return { width, height: cy + GAP - y };
  };

  const main = list(flow.steps, { list: "top" }, MARGIN, MARGIN, false);
  let width = MARGIN + main.width;
  let height = MARGIN + main.height;

  // Detached steps float where they were put (or in a column to the right).
  const positions = flow.editor?.positions ?? {};
  let freeY = MARGIN;
  const freeX = width + 120;
  for (const step of flow.editor?.detached ?? []) {
    const at = positions[step.id];
    const x = at ? at[0] : freeX;
    const y = at ? at[1] : freeY;
    const laid = list([step], null, x, y - GAP, true);
    if (!at) freeY += laid.height;
    width = Math.max(width, x + laid.width);
    height = Math.max(height, y - GAP + laid.height);
  }
  return { nodes, edges, ports, width: width + MARGIN, height: height + MARGIN };
}

/** Every step in the flow (detached ones included), by ID. */
export function stepsById(flow: Flow): Map<string, FlowStep> {
  const out = new Map<string, FlowStep>();
  const visit = (step: FlowStep) => {
    out.set(step.id, step);
    for (const sub of listsOf(step)) sub.steps.forEach(visit);
  };
  flow.steps.forEach(visit);
  (flow.editor?.detached ?? []).forEach(visit);
  return out;
}

/** Whether `id` is a detached step (or inside one). */
export function isDetached(flow: Flow, id: string): boolean {
  const inside = (steps: FlowStep[]): boolean =>
    steps.some((s) => s.id === id || listsOf(s).some((l) => inside(l.steps)));
  return inside(flow.editor?.detached ?? []) && !inside(flow.steps);
}

/** Where step `id` sits: its list and index (connected steps only). */
export function positionOf(flow: Flow, id: string): FlowPosition | null {
  const search = (steps: FlowStep[], slot: Slot): FlowPosition | null => {
    for (let index = 0; index < steps.length; index++) {
      const step = steps[index];
      if (step.id === id) return { ...slot, index };
      for (const sub of listsOf(step)) {
        const found = search(sub.steps, sub.slot);
        if (found) return found;
      }
    }
    return null;
  };
  return search(flow.steps, { list: "top" });
}
