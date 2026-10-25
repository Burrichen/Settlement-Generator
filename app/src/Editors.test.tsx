// Tag and table editors: schema-driven forms, coverage feedback, the save
// gate and the unsaved-changes guard. The core is mocked at `api`.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import type {
  ContentOverview,
  Coverage,
  DiceAdvice,
  DiceFit,
  DraftCheck,
  EffectKindSchema,
  Settings,
  Table,
  TableDetail,
  Tag,
  TagDetail,
} from "./api";

const settings: Settings = {
  generation_method: "automatic",
  generation_presentation: "incremental",
  god_mode: false,
  tag_view: false,
  theme: "modern_dark",
  bare_minimum_buildings: false,
};

const d20: Table = {
  id: "table.econ",
  name: "Economy",
  dice: "1d20",
  results: [
    { id: "poor", name: "Poor", range: [1, 10], tags: ["g.poor"] },
    { id: "rich", name: "Rich", range: [11, 20] },
  ],
};

const custom: Table = {
  id: "user.table.mine",
  name: "Mine",
  dice: "1d4",
  results: [{ id: "user.r", name: "Only", range: [1, 4] }],
};

function coverage(uncovered: boolean): Coverage {
  return {
    label: "Base layout",
    dice: "1d20",
    faces: Array.from({ length: 20 }, (_, i) => ({
      face: i + 1,
      chance: 0.05,
      claimed_by: uncovered && i === 19 ? [] : [i < 10 ? "poor" : "rich"],
    })),
    results: [
      { result: "poor", range: [1, 10], chance: 0.5 },
      { result: "rich", range: [11, uncovered ? 19 : 20], chance: uncovered ? 0.45 : 0.5 },
    ],
    uncovered: uncovered ? [20] : [],
    overlapping: [],
    impossible: [],
  };
}

const tableDetail = (table: Table, origin: TableDetail["origin"] = "bundled"): TableDetail => ({
  table,
  origin,
  built_in: !table.id.startsWith("user."),
  coverage: [coverage(false)],
  issues: [],
});

const poorTag: Tag = { id: "g.poor", name: "Poor", effects: [] };

const schema: EffectKindSchema[] = [
  {
    kind: "add_tag",
    label: "Add tag",
    description: "Also emit another tag.",
    executable: true,
    group: "Change the roll",
    fields: [{ name: "tag", label: "Tag", kind: { type: "tag" }, required: true, help: "" }],
  },
  {
    kind: "secondary_roll",
    label: "Secondary roll",
    description: "Roll extra dice.",
    executable: true,
    group: "Change the roll",
    fields: [
      { name: "label", label: "Label", kind: { type: "text" }, required: true, help: "" },
      { name: "dice", label: "Dice", kind: { type: "dice" }, required: true, help: "" },
    ],
  },
];

let overview: ContentOverview;
let nextCheck: DraftCheck;
let keptDraft: Table | null = null;

const d10: DiceFit = {
  dice: "1d10",
  kind: "exact",
  label: "1d10, 5 faces each",
  explanation: "Roll 1d10. Each result has 5 faces, so each has exactly 50%.",
  ranges: [
    [1, 5],
    [6, 10],
  ],
  chances: [0.5, 0.5],
  max_error: 0,
};

const advice: DiceAdvice = {
  targets: [0.5, 0.5],
  suggestions: [
    {
      ...d10,
      dice: "1d4",
      label: "1d4, 2 faces each",
      ranges: [
        [1, 2],
        [3, 4],
      ],
    },
    d10,
  ],
  summary: "2 equal results fit 1d4 exactly: 1d4, 2 faces each.",
};

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "0.1.0" })),
    getRoadmap: vi.fn(async () => []),
    getSettings: vi.fn(async () => settings),
    saveSettings: vi.fn(async (_previous: Settings, s: Settings) => s),
    exitApp: vi.fn(),
    content: {
      overview: vi.fn(async () => overview),
      table: vi.fn(async (id: string) =>
        tableDetail(id === custom.id ? custom : d20, id === custom.id ? "custom" : "bundled"),
      ),
      tag: vi.fn(async (): Promise<TagDetail> => ({
        tag: poorTag,
        origin: "bundled",
        built_in: true,
        usages: [
          {
            kind: "emitted_by",
            location: { table: "table.econ", result: "poor" },
            description: "emitted by table `table.econ` › result `poor`",
          },
        ],
        issues: [],
      })),
      checkTable: vi.fn(async () => nextCheck),
      checkTag: vi.fn(async () => ({ coverage: [], issues: [], error: null })),
      saveTable: vi.fn(async (t: Table) => tableDetail(t, "modified")),
      saveTag: vi.fn(async (t: Tag): Promise<TagDetail> => ({
        tag: t,
        origin: "modified",
        built_in: true,
        usages: [],
        issues: [],
      })),
      newTable: vi.fn(async () => custom),
      newTag: vi.fn(async () => ({ id: "user.tag.new", name: "New tag" })),
      newId: vi.fn(async (kind: string) => `user.${kind}.1`),
      deleteTable: vi.fn(async () => overview),
      deleteTag: vi.fn(async () => overview),
      resetTable: vi.fn(async () => overview),
      resetTag: vi.fn(async () => overview),
      resetAll: vi.fn(async () => overview),
      effectSchema: vi.fn(async () => schema),
      diceAdvice: vi.fn(async () => advice),
      diceFit: vi.fn(async () => d10),
      applyDiceFit: vi.fn(async (t: Table, entries: string[], fit: DiceFit) => ({
        ...t,
        dice: fit.dice,
        results: t.results.map((r) => {
          const i = entries.indexOf(r.id);
          return { ...r, range: i < 0 ? undefined : fit.ranges[i] };
        }),
      })),
      previewRoll: vi.fn(async (t: Table) => ({
        events: [
          {
            event: "table_rolled",
            table: { id: t.id, name: t.name },
            dice: t.dice,
            attempts: [
              {
                roll: { total: 14, dice: [] },
                face: 14,
                result: { id: "rich", name: "Rich" },
                outcome: "accepted",
              },
            ],
            result: { id: "rich", name: "Rich" },
          },
        ],
        traces: [{ title: t.name, lines: ["Base roll: 14 (1d20: 14)", "Result: Rich"] }],
        note: "A trial roll: each die is rolled separately.",
      })),
      copyTable: vi.fn(async () => ({ ...d20, id: "user.table.copy", name: "Copy of Economy" })),
      resultUsages: vi.fn(async (_t: Table, result: string) =>
        result === "poor" ? [{ description: "A restriction of tag “Rich”" }] : [],
      ),
      saveTableDraft: vi.fn(async () => {}),
      tableDraft: vi.fn(async () => keptDraft),
      discardTableDraft: vi.fn(async () => {}),
    },
    framework: {
      ...actual.framework,
      items: vi.fn(async () => [
        { kind: "shared_result", id: "env.wood", name: "Woodland", deleted: false },
      ]),
      saveItem: vi.fn(async () => [
        { kind: "shared_result", id: "env.wood", name: "Woodland", deleted: false },
      ]),
      settings: vi.fn(async () => []),
      toggleSettings: vi.fn(async () => []),
    },
  };
});

import { App } from "./App";
import { content } from "./api";

const api = vi.mocked(content);

async function openEditor(which: "View Tables" | "View Tags") {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.1.0");
  await user.keyboard("{ArrowDown}{Enter}"); // Settings
  await user.click(await screen.findByRole("button", { name: `Open ${which}` }));
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  keptDraft = null;
  nextCheck = { coverage: [coverage(false)], issues: [], error: null };
  overview = {
    tables: [
      {
        id: d20.id,
        name: d20.name,
        origin: "bundled",
        built_in: true,
        deleted: false,
        active: true,
        incomplete: false,
        errors: 0,
      },
      {
        id: custom.id,
        name: custom.name,
        origin: "custom",
        built_in: false,
        deleted: false,
        active: true,
        incomplete: false,
        errors: 0,
      },
    ],
    tags: [
      {
        id: "g.poor",
        name: "Poor",
        origin: "bundled",
        built_in: true,
        deleted: false,
        enabled: true,
      },
    ],
    issues: [],
  };
});

afterEach(cleanup);

describe("table editor", () => {
  it("lists effective tables and opens the first in a generic form", async () => {
    await openEditor("View Tables");
    const list = await screen.findByRole("listbox", { name: "Tables" });
    expect(within(list).getAllByRole("option")).toHaveLength(2);
    expect(await screen.findByDisplayValue("Economy")).toBeTruthy();
    expect(screen.getByDisplayValue("1d20")).toBeTruthy();
    expect(screen.getByText("Every face is covered exactly once")).toBeTruthy();
    // Results show their chance under the physical dice.
    expect(screen.getAllByText("50%")).toHaveLength(2);
  });

  it("highlights uncovered faces and refuses to save until they are fixed", async () => {
    const user = await openEditor("View Tables");
    const to = await screen.findByLabelText("Rich faces: to");
    nextCheck = {
      coverage: [coverage(true)],
      issues: [
        {
          severity: "error",
          location: { table: "table.econ" },
          code: "uncovered_outcomes",
          message: "table `table.econ`: no result for 20 on 1d20",
        },
      ],
      error: null,
    };
    await user.clear(to);
    await user.type(to, "19");
    // The Dice Assistant says so in plain English…
    expect(await screen.findByText("No result is on face 20.")).toBeTruthy();
    // …and every face is shown under Advanced.
    await user.click(screen.getByRole("button", { name: /Advanced/ }));
    expect(await screen.findByText(/Uncovered: 20/)).toBeTruthy();
    const face20 = screen.getByTitle(/^20: no result/);
    expect(face20.className).toContain("face-uncovered");
    expect(await screen.findByText(/no result for 20 on 1d20/)).toBeTruthy();
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    nextCheck = { coverage: [coverage(false)], issues: [], error: null };
    await user.clear(to);
    await user.type(to, "20");
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(api.saveTable).toHaveBeenCalledWith(
      expect.objectContaining({ id: "table.econ", results: expect.any(Array) }),
    );
    expect(await screen.findByText("Saved")).toBeTruthy();
  });

  it("shows and edits result values only on tables that use them", async () => {
    const counts: Table = {
      id: "table.counts",
      name: "Counts",
      dice: "1d20",
      results: [
        { id: "poor", name: "Two", range: [1, 10], value: 2 },
        { id: "rich", name: "Three", range: [11, 20], value: 3 },
      ],
    };
    const original = api.table.getMockImplementation();
    api.table.mockImplementation(async () => tableDetail(counts));
    onTestFinished(() => {
      api.table.mockImplementation(original!);
    });
    const user = await openEditor("View Tables");
    const value = (await screen.findByLabelText("Three: value")) as HTMLInputElement;
    expect(value.value).toBe("3");
    await user.clear(value);
    await user.type(value, "5");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(api.saveTable).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [
          expect.objectContaining({ id: "poor", value: 2 }),
          expect.objectContaining({ id: "rich", value: 5 }),
        ],
      }),
    );
  });

  it("edits unique roles and their fallbacks in the result's role options", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("Economy");
    await user.click(screen.getByRole("button", { name: "Rich: advanced options" }));
    await user.click(screen.getByLabelText("Rich: unique"));
    const fallback = screen.getByLabelText("Rich: fallback") as HTMLSelectElement;
    // Only other, repeatable results can be a fallback.
    expect(within(fallback).queryByText("Rich")).toBeNull();
    await user.selectOptions(fallback, "poor");
    await user.click(screen.getByRole("button", { name: "Add display variant" }));
    await user.type(screen.getByLabelText("Rich: display variant 1 label"), "Wealthy");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(api.saveTable).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [
          expect.objectContaining({ id: "poor" }),
          expect.objectContaining({
            id: "rich",
            unique: true,
            fallback: "poor",
            label_variants: [{ tag: "", name: "Wealthy" }],
          }),
        ],
      }),
    );
  });

  it("has no value column on tables without values", async () => {
    await openEditor("View Tables");
    await screen.findByDisplayValue("Economy");
    expect(screen.queryByRole("columnheader", { name: "Value" })).toBeNull();
  });

  it("adds results, variants and follow-up rolls with generated IDs", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("Economy");
    await user.click(screen.getByRole("button", { name: "Add result" }));
    expect(await screen.findByDisplayValue("New result")).toBeTruthy();
    // Variants and follow-up rolls are advanced options, collapsed at first.
    expect(screen.queryByRole("button", { name: "Add variant" })).toBeNull();
    await user.click(screen.getByRole("button", { name: /Advanced/ }));
    await user.click(screen.getByRole("button", { name: "Add variant" }));
    expect(screen.getByDisplayValue("New variant")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add follow-up roll" }));
    expect(screen.getByDisplayValue("New follow-up roll")).toBeTruthy();
    expect(api.newId).toHaveBeenCalledWith("result");
    expect(api.newId).toHaveBeenCalledWith("variant");
    expect(api.newId).toHaveBeenCalledWith("followUp");
  });

  it("offers Delete only for custom tables, with confirmation", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("Economy");
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
    // Unmodified defaults have nothing to reset.
    expect(screen.queryByRole("button", { name: "Reset to default" })).toBeNull();

    await user.click(screen.getByRole("option", { name: /Mine/ }));
    await screen.findByDisplayValue("Mine");
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(api.deleteTable).not.toHaveBeenCalled();
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }),
    );
    expect(api.deleteTable).toHaveBeenCalledWith("user.table.mine");
  });

  it("asks before discarding unsaved changes", async () => {
    const user = await openEditor("View Tables");
    const name = await screen.findByDisplayValue("Economy");
    await user.type(name, "!");
    await user.click(screen.getByRole("option", { name: /Mine/ }));
    expect(screen.getByText("Discard your unsaved changes?")).toBeTruthy();
    expect(screen.getByDisplayValue("Economy!")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(await screen.findByDisplayValue("Mine")).toBeTruthy();
  });

  it("shows only the basics until Advanced is opened", async () => {
    await openEditor("View Tables");
    expect(await screen.findByLabelText("Table name")).toBeTruthy();
    expect(screen.getByRole("radiogroup", { name: "Chance mode" })).toBeTruthy();
    expect(screen.getByDisplayValue("1d20")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Dice Assistant" })).toBeTruthy();
    expect(screen.queryByText("How results are found")).toBeNull();
    expect(screen.queryByText("Emits tags")).toBeNull();
  });

  it("applies a Dice Assistant suggestion and can undo and redo it", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    expect(await screen.findByText(/2 equal results fit 1d4 exactly/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Use 1d4, 2 faces each" }));
    expect(await screen.findByDisplayValue("1d4")).toBeTruthy();
    expect(api.applyDiceFit).toHaveBeenCalledWith(
      expect.objectContaining({ id: "table.econ" }),
      ["poor", "rich"],
      expect.objectContaining({ dice: "1d4" }),
    );
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(await screen.findByDisplayValue("1d20")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(await screen.findByDisplayValue("1d4")).toBeTruthy();
  });

  it("lays results out again when a row is added in Equal mode", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    await user.click(screen.getByRole("button", { name: "Add result" }));
    await waitFor(() => expect(api.diceFit).toHaveBeenCalledWith([1, 1, 1], "1d20"));
  });

  it("will not delete a result something depends on", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    await user.click(screen.getByRole("button", { name: "Remove Poor" }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText(/cannot be deleted while it is used by: A restriction/)).toBeTruthy();
    expect(screen.getByDisplayValue("Poor")).toBeTruthy();
    // Rich is unused, so it goes.
    await user.click(screen.getByRole("button", { name: "Remove Rich" }));
    await waitFor(() => expect(screen.queryByDisplayValue("Rich")).toBeNull());
  });

  it("duplicates and reorders rows", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    await user.click(screen.getByRole("button", { name: "Duplicate Poor" }));
    expect(await screen.findByDisplayValue("Copy of Poor")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Move Rich up" }));
    await waitFor(() => {
      const names = screen
        .getAllByRole("textbox", { name: /: name$/ })
        .map((i) => (i as HTMLInputElement).value);
      expect(names).toEqual(["Poor", "Rich", "Copy of Poor"]);
    });
  });

  it("previews a roll and tests a dice total", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    // Roll once explains the roll; Step through reveals it line by line.
    await user.click(screen.getByRole("button", { name: "Roll once" }));
    const once = await screen.findByRole("list", { name: "Roll once" });
    expect(within(once).getByText("Result: Rich")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Step through" }));
    const steps = await screen.findByRole("list", { name: "Step through" });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Next step" }));
    expect(within(steps).getAllByRole("listitem")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Show all" }));
    expect(within(steps).getByText("Result: Rich")).toBeTruthy();
    await user.type(screen.getByLabelText("Dice total to test"), "7");
    expect(await screen.findByText("7 gives Poor (5% to roll exactly 7).")).toBeTruthy();
    await user.clear(screen.getByLabelText("Dice total to test"));
    await user.type(screen.getByLabelText("Dice total to test"), "25");
    expect(await screen.findByText("1d20 cannot roll 25.")).toBeTruthy();
  });

  it("offers a draft kept from earlier and keeps new changes as a draft", async () => {
    keptDraft = { ...d20, name: "Economy (draft)" };
    const user = await openEditor("View Tables");
    await screen.findByText(/unsaved changes to this table from earlier/);
    await user.click(screen.getByRole("button", { name: "Restore them" }));
    expect(await screen.findByDisplayValue("Economy (draft)")).toBeTruthy();
    await waitFor(() => expect(api.saveTableDraft).toHaveBeenCalled(), { timeout: 2000 });
  });

  it("copies a table into a new one", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    await user.click(screen.getByRole("button", { name: "Copy to a new table" }));
    expect(await screen.findByDisplayValue("Copy of Economy")).toBeTruthy();
    expect(screen.getByText(/New custom table \(not saved\)/)).toBeTruthy();
  });

  it("links a result to a shared result", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("1d20");
    // The shared results load separately; wait for them.
    await within(screen.getByLabelText("Poor: shared result")).findByRole("option", {
      name: "Woodland",
    });
    await user.selectOptions(screen.getByLabelText("Poor: shared result"), "env.wood");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(api.saveTable).toHaveBeenCalledWith(
      expect.objectContaining({
        results: expect.arrayContaining([
          expect.objectContaining({ id: "poor", shared: "env.wood", name: "" }),
        ]),
      }),
    );
  });

  it("restores all defaults after confirmation", async () => {
    const user = await openEditor("View Tables");
    await screen.findByDisplayValue("Economy");
    await user.click(screen.getByRole("button", { name: "Restore all defaults" }));
    await user.click(screen.getByRole("button", { name: "Restore all" }));
    expect(api.resetAll).toHaveBeenCalled();
  });
});

describe("tag editor", () => {
  it("shows the stable ID and where the tag is used", async () => {
    await openEditor("View Tags");
    expect(await screen.findByDisplayValue("Poor")).toBeTruthy();
    expect(screen.getAllByText("g.poor").length).toBeGreaterThan(0);
    expect(screen.getByText("emitted by table `table.econ` › result `poor`")).toBeTruthy();
  });

  it("renders effect forms from the core's schema", async () => {
    const user = await openEditor("View Tags");
    await screen.findByDisplayValue("Poor");
    await user.selectOptions(screen.getByLabelText("Add effect"), "secondary_roll");
    expect(screen.getByLabelText("Effect 1: Label")).toBeTruthy();
    expect(screen.getByLabelText("Effect 1: Dice")).toBeTruthy();
    // Changing the kind swaps the fields.
    await user.selectOptions(screen.getByLabelText("Effect 1: kind"), "add_tag");
    expect(screen.queryByLabelText("Effect 1: Dice")).toBeNull();
    expect(screen.getByLabelText("Effect 1: Tag")).toBeTruthy();
  });

  it("resets a customised default tag to its installed version", async () => {
    const modified: TagDetail = {
      tag: { ...poorTag, name: "Destitute" },
      origin: "modified",
      built_in: true,
      usages: [],
      issues: [],
    };
    api.tag.mockResolvedValueOnce(modified);
    const user = await openEditor("View Tags");
    expect(await screen.findByDisplayValue("Destitute")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Reset to default" }));
    expect(api.resetTag).toHaveBeenCalledWith("g.poor");
    // Reloaded from the core: the installed name is back.
    expect(await screen.findByDisplayValue(poorTag.name)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Reset to default" })).toBeNull();
  });

  it("renames by ID without touching references", async () => {
    const user = await openEditor("View Tags");
    const name = await screen.findByDisplayValue("Poor");
    await user.clear(name);
    await user.type(name, "Impoverished");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    expect(api.saveTag).toHaveBeenCalledWith(
      expect.objectContaining({ id: "g.poor", name: "Impoverished" }),
    );
  });
});
