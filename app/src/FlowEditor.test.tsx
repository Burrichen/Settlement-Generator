// The visual flow editor: layout, the plain-language condition builder, and
// the editor itself (nodes, adding steps, settings, undo/redo, drafts,
// search, saving). The core is mocked at `api`; its edits are faked here.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Condition, Flow, FlowCheck, FlowContext, FlowEdit, FlowStep, Settings } from "./api";
import { ConditionBuilder } from "./components/flows/ConditionBuilder";
import { layoutFlow } from "./components/flows/layout";

afterEach(cleanup);

const context: FlowContext = {
  subjects: [
    {
      step: "authority",
      label: "Authority",
      results: [
        { id: "military", name: "Military Authority" },
        { id: "civil", name: "Civil Authority" },
      ],
      numeric: false,
      pipeline: false,
    },
    {
      step: "representation",
      label: "Representation",
      results: [
        { id: "single", name: "Single" },
        { id: "group", name: "Group" },
      ],
      numeric: false,
      pipeline: false,
    },
    { step: "count", label: "Count", results: [], numeric: true, pipeline: false },
  ],
  tables: [{ id: "table.authority", name: "Authority" }],
  tags: [{ id: "tag.lucky", name: "Lucky" }],
  settings: [
    {
      id: "setting.bareMinimumBuildings",
      name: "Bare Minimum Buildings",
      kind: "toggle",
      options: [],
    },
  ],
  objects: [{ id: "object.figure", name: "Figure" }],
  flows: [{ id: "flow.size", name: "Authority Size" }],
  kinds: [
    { id: "roll_table", name: "Roll Table" },
    { id: "condition", name: "If / Condition" },
    { id: "repeat_times", name: "Repeat N Times" },
    { id: "stop", name: "Stop / End" },
  ],
};

// ----- layout ---------------------------------------------------------------------------

describe("flow layout", () => {
  const flow: Flow = {
    id: "flow.x",
    name: "X",
    steps: [
      { id: "a", kind: "roll", table: "t" },
      {
        id: "if",
        kind: "branch",
        cases: [{ when: { all: [] }, steps: [{ id: "b", kind: "stop" }] }],
      },
    ],
    editor: { detached: [{ id: "loose", kind: "stop" }], positions: { loose: [900, 300] } },
  };

  it("gives every step a node and every gap a port", () => {
    const laid = layoutFlow(flow, () => "always");
    expect(laid.nodes.map((n) => n.id)).toEqual(["a", "if", "b", "loose"]);
    const keys = laid.ports.map((p) => p.key);
    // Top: before a, before if, after if. Case 1: before and after b.
    // Otherwise: empty, one port.
    expect(keys).toEqual([
      "top:0",
      "top:1",
      "case:if:0:0",
      "case:if:0:1",
      "otherwise:if:0",
      "top:2",
    ]);
    expect(laid.edges.find((e) => e.id === "into:case:if:0")?.label).toBe("IF always");
  });

  it("keeps a detached step where it was put, with no ports", () => {
    const laid = layoutFlow(flow);
    const loose = laid.nodes.find((n) => n.id === "loose")!;
    expect([loose.x, loose.y]).toEqual([900, 300]);
    expect(loose.detached).toBe(true);
    expect(laid.ports.some((p) => "step" in p.at && p.at.step === "loose")).toBe(false);
  });
});

// ----- condition builder --------------------------------------------------------------

describe("condition builder", () => {
  const rule: Condition = {
    all: [
      { result_is: { step: "authority", results: ["military"] } },
      { result_is: { step: "representation", results: ["group"] } },
    ],
  };

  it("reads as WHEN … AND …", () => {
    render(<ConditionBuilder value={rule} onChange={() => {}} context={context} label="Rule" />);
    expect(screen.getByText("WHEN")).toBeTruthy();
    expect(screen.getByText("AND")).toBeTruthy();
    const subject = screen.getByRole("combobox", { name: "Rule test 1 subject" });
    expect((subject as HTMLSelectElement).value).toBe("step:authority");
    const value = screen.getByRole("combobox", { name: "Rule test 1 value" });
    expect((value as HTMLSelectElement).selectedOptions[0].textContent).toBe("Military Authority");
  });

  it("negates a test and changes how tests join", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<ConditionBuilder value={rule} onChange={onChange} context={context} label="Rule" />);
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Rule test 2 operator" }),
      "is_not",
    );
    expect(onChange).toHaveBeenLastCalledWith({
      all: [
        { result_is: { step: "authority", results: ["military"] } },
        { not: { result_is: { step: "representation", results: ["group"] } } },
      ],
    });
    await user.selectOptions(screen.getByRole("combobox", { name: "Rule match" }), "none");
    expect(onChange).toHaveBeenLastCalledWith({ not: { any: rule.all } });
  });

  it("offers number tests for a step that stores a number", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <ConditionBuilder
        value={{ value_in: { step: "count", min: 2, max: 4 } }}
        onChange={onChange}
        context={context}
        label="Rule"
      />,
    );
    expect(
      (screen.getByRole("combobox", { name: "Rule test 1 operator" }) as HTMLSelectElement).value,
    ).toBe("between");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Rule test 1 operator" }),
      "at_least",
    );
    expect(onChange).toHaveBeenLastCalledWith({ value_in: { step: "count", min: 2 } });
  });

  it("builds settings and tag tests, and nested groups", async () => {
    const user = userEvent.setup();
    let value: Condition = { all: [] };
    const { rerender } = render(
      <ConditionBuilder
        value={value}
        onChange={(c) => (value = c)}
        context={context}
        label="Rule"
      />,
    );
    expect(screen.getByText("Always applies.")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add a test" }));
    rerender(
      <ConditionBuilder
        value={value}
        onChange={(c) => (value = c)}
        context={context}
        label="Rule"
      />,
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Rule test 1 subject" }),
      "setting:setting.bareMinimumBuildings",
    );
    expect(value).toEqual({ setting_on: "setting.bareMinimumBuildings" });
    rerender(
      <ConditionBuilder
        value={value}
        onChange={(c) => (value = c)}
        context={context}
        label="Rule"
      />,
    );
    await user.click(screen.getByRole("button", { name: "Add group" }));
    expect(value).toEqual({
      all: [
        { setting_on: "setting.bareMinimumBuildings" },
        { result_is: { step: "authority", results: ["military"] } },
      ],
    });
  });
});

// ----- the editor -------------------------------------------------------------------------

const settings: Settings = {
  generation_method: "manual",
  generation_presentation: "incremental",
  theme: "modern_dark",
  tag_view: false,
  debug_mode: false,
  god_mode: false,
  bare_minimum_buildings: false,
  auto_check_updates: false,
} as unknown as Settings;

let stored: Flow;
let checkErrors = false;
let keptDraft: Flow | null = null;

function baseFlow(): Flow {
  return {
    id: "flow.figures",
    name: "Figures",
    steps: [
      { id: "authority", kind: "roll", table: "table.authority" },
      {
        id: "size",
        kind: "branch",
        cases: [
          {
            when: { result_is: { step: "authority", results: ["military"] } },
            steps: [{ id: "run-size", kind: "run_flow", flow: "flow.size" }],
          },
        ],
      },
    ],
  };
}

function nodeText(step: FlowStep): string {
  switch (step.kind) {
    case "roll":
      return "Roll Authority";
    case "branch":
      return "Choose";
    case "run_flow":
      return "Run Authority Size";
    case "repeat":
      return "Repeat 1 time";
    default:
      return step.kind;
  }
}

function allSteps(steps: FlowStep[]): FlowStep[] {
  return steps.flatMap((s) => [
    s,
    ...(s.kind === "branch"
      ? s.cases.flatMap((c) => allSteps(c.steps))
      : s.kind === "repeat"
        ? allSteps(s.steps)
        : []),
  ]);
}

/** A small stand-in for the core's edits, enough for these tests. */
function fakeEdit(flow: Flow, edit: FlowEdit): Flow {
  const next: Flow = structuredClone(flow);
  switch (edit.op) {
    case "add": {
      const id = edit.kind === "repeat_times" ? "repeat" : edit.kind;
      const step: FlowStep =
        edit.kind === "repeat_times"
          ? { id, kind: "repeat", count: { times: 1 }, steps: [] }
          : { id, kind: "stop" };
      next.steps.splice(edit.at.index, 0, step);
      return next;
    }
    case "duplicate": {
      const i = next.steps.findIndex((s) => s.id === edit.step);
      next.steps.splice(i + 1, 0, { ...next.steps[i], id: `${edit.step}-2` });
      return next;
    }
    case "remove":
      next.steps = next.steps.filter((s) => s.id !== edit.step);
      return next;
    default:
      return next;
  }
}

function fakeCheck(flow: Flow): FlowCheck {
  return {
    issues: checkErrors
      ? [
          {
            severity: "error",
            location: { flow: flow.id, step: "authority" },
            code: "missing_table",
            message: "step `authority`: there is no table `table.ghost`",
          },
        ]
      : [],
    pseudocode: allSteps(flow.steps).map((s) => ({
      depth: 0,
      step: s.id,
      text: `${nodeText(s)}.`,
    })),
    nodes: allSteps(flow.steps).map((s) => ({
      step: s.id,
      kind: s.kind === "roll" ? "Roll Table" : s.kind === "branch" ? "If / Condition" : s.kind,
      text: nodeText(s),
      cases: s.kind === "branch" ? ["Authority is Military Authority"] : undefined,
    })),
    error: null,
  };
}

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "0.1.0" })),
    getRoadmap: vi.fn(async () => []),
    getSettings: vi.fn(async () => settings),
    saveSettings: vi.fn(async (_p: Settings, s: Settings) => s),
    exitApp: vi.fn(),
    framework: {
      ...actual.framework,
      items: vi.fn(async () => [
        { kind: "flow", id: "flow.figures", name: "Figures", origin: "bundled", deleted: false },
      ]),
      settings: vi.fn(async () => []),
      toggleSettings: vi.fn(async () => []),
      deleteItem: vi.fn(async () => []),
      resetItem: vi.fn(async () => []),
    },
    flows: {
      get: vi.fn(async () => structuredClone(stored)),
      create: vi.fn(async () => ({ id: "user.flow.new", name: "New flow", steps: [] })),
      duplicate: vi.fn(async () => ({
        ...structuredClone(stored),
        id: "user.flow.copy",
        name: "Copy of Figures",
      })),
      edit: vi.fn(async (flow: Flow, edit: FlowEdit) => fakeEdit(flow, edit)),
      check: vi.fn(async (flow: Flow) => fakeCheck(flow)),
      context: vi.fn(async () => context),
      describeCondition: vi.fn(async () => ""),
      usages: vi.fn(async () => []),
      remove: vi.fn(async () => []),
      preview: vi.fn(async () => ({ steps: [], outline: [], traces: [] })),
      simulate: vi.fn(),
      save: vi.fn(async (flow: Flow) => {
        stored = structuredClone(flow);
        return flow;
      }),
      saveDraft: vi.fn(async () => {}),
      draft: vi.fn(async () => keptDraft),
      discardDraft: vi.fn(async () => {}),
    },
  };
});

import { App } from "./App";
import { flows } from "./api";

const api = vi.mocked(flows);

async function openFlows() {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.1.0");
  await user.keyboard("{ArrowDown}{Enter}"); // Settings
  await user.click(await screen.findByRole("button", { name: "Open View Flows" }));
  await screen.findByRole("button", { name: "Roll Table: Roll Authority" });
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  stored = baseFlow();
  checkErrors = false;
  keptDraft = null;
});

describe("flow editor", () => {
  it("shows the flow as nodes, with its pseudocode", async () => {
    await openFlows();
    expect(screen.getByRole("button", { name: "If / Condition: Choose" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "run_flow: Run Authority Size" })).toBeTruthy();
    const code = screen.getByRole("list", { name: "Pseudocode" });
    expect(
      within(code)
        .getAllByRole("listitem")
        .map((l) => l.textContent),
    ).toEqual(["Roll Authority.", "Choose.", "Run Authority Size."]);
    // The branch's case is labelled on its edge.
    expect(screen.getAllByText("IF Authority is Military Authority").length).toBeGreaterThan(0);
  });

  it("adds a step from a + and selects it", async () => {
    const user = await openFlows();
    const ports = screen.getAllByRole("button", { name: "Add a step here" });
    await user.click(ports[ports.length - 1]);
    await user.click(screen.getByRole("menuitem", { name: "Repeat N Times" }));
    expect(api.edit).toHaveBeenCalledWith(expect.anything(), {
      op: "add",
      kind: "repeat_times",
      at: { list: "top", index: 2 },
    });
    await screen.findByRole("button", { name: "repeat: Repeat 1 time" }, { timeout: 3000 });
    expect(
      await screen.findByRole("radiogroup", { name: "How many times" }, { timeout: 3000 }),
    ).toBeTruthy();
  });

  it("edits a case condition in plain words and undoes it", async () => {
    const user = await openFlows();
    await user.click(screen.getByRole("button", { name: "If / Condition: Choose" }));
    const operator = await screen.findByRole("combobox", {
      name: "Case 1 condition test 1 operator",
    });
    await user.selectOptions(operator, "is_not");
    await waitFor(() =>
      expect(api.check).toHaveBeenLastCalledWith(
        expect.objectContaining({
          steps: expect.arrayContaining([
            expect.objectContaining({
              id: "size",
              cases: [
                expect.objectContaining({
                  when: { not: { result_is: { step: "authority", results: ["military"] } } },
                }),
              ],
            }),
          ]),
        }),
      ),
    );
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(
      (
        screen.getByRole("combobox", {
          name: "Case 1 condition test 1 operator",
        }) as HTMLSelectElement
      ).value,
    ).toBe("is");
    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(
      (
        screen.getByRole("combobox", {
          name: "Case 1 condition test 1 operator",
        }) as HTMLSelectElement
      ).value,
    ).toBe("is_not");
  });

  it("keeps unsaved changes as a draft and saves", async () => {
    const user = await openFlows();
    const name = screen.getByRole("textbox", { name: "Flow name" });
    await user.clear(name);
    await user.type(name, "People");
    await waitFor(() => expect(api.saveDraft).toHaveBeenCalled(), { timeout: 2000 });
    await user.click(screen.getByRole("button", { name: "Save flow" }));
    await waitFor(() =>
      expect(api.save).toHaveBeenCalledWith(expect.objectContaining({ name: "People" })),
    );
    expect(await screen.findByText("Flow saved.")).toBeTruthy();
  });

  it("refuses to save while the flow has errors, and lists them", async () => {
    checkErrors = true;
    const user = await openFlows();
    await user.type(screen.getByRole("textbox", { name: "Flow name" }), "!");
    await screen.findByText(/1 error, 0 warnings/);
    expect((screen.getByRole("button", { name: "Save flow" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
    await user.click(screen.getByRole("tab", { name: /Problems/ }));
    await user.click(screen.getByRole("button", { name: /there is no table/ }));
    expect(
      screen
        .getByRole("button", { name: "Roll Table: Roll Authority" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
  });

  it("finds steps, duplicates and deletes them", async () => {
    const user = await openFlows();
    await user.type(screen.getByRole("searchbox", { name: "Find a step" }), "authority");
    expect(screen.getByText("2 matches")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Roll Table: Roll Authority" }));
    await user.click(await screen.findByRole("button", { name: "Duplicate" }));
    expect(api.edit).toHaveBeenCalledWith(expect.anything(), {
      op: "duplicate",
      step: "authority",
    });
    await user.click(await screen.findByRole("button", { name: "Delete step" }));
    expect(api.edit).toHaveBeenLastCalledWith(expect.anything(), {
      op: "remove",
      step: "authority-2",
    });
  });

  it("offers a draft kept from earlier", async () => {
    keptDraft = { ...baseFlow(), name: "Figures (draft)" };
    await openFlows();
    expect(screen.getByText("Unsaved changes from earlier were restored.")).toBeTruthy();
    expect((screen.getByRole("textbox", { name: "Flow name" }) as HTMLInputElement).value).toBe(
      "Figures (draft)",
    );
  });
});
