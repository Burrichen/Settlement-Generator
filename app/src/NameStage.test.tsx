// The name stage in the GUI: generated names, method choice, rerolling the
// name alone, typed names, the no-scheme fallback and Tag View details. The
// core is replaced at the `api` boundary; naming rules are tested in Rust.
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GenerationResponse, NamingView, SessionView, Settings, StageView } from "./api";

const methods = [
  {
    result: { id: "geographic", name: "Geographic Compound" },
    tags: [],
    legal: true,
    range: [1, 7] as [number, number],
  },
  {
    result: { id: "founder", name: "Historical Founder" },
    tags: [],
    legal: true,
    range: [8, 10] as [number, number],
  },
];

function tableStage(index: number, name: string, result: string): StageView {
  return {
    index,
    stage: { id: `table.${index}`, name },
    table: { id: `table.${index}`, name },
    status: "resolved",
    current: false,
    actionable: true,
    can_roll: true,
    dice: "1d8",
    options: [{ result: { id: result.toLowerCase(), name: result }, tags: [], legal: true }],
    result: { id: result.toLowerCase(), name: result },
  };
}

function nameStage(naming: NamingView, extra: Partial<StageView> = {}): StageView {
  const generated = naming.source === "generated";
  return {
    index: 2,
    stage: { id: "stage.name", name: "Name" },
    table: { id: "table.naming.ordon.method", name: "Ordon Naming Method" },
    status: naming.name ? "resolved" : "pending",
    current: true,
    actionable: true,
    can_roll: true,
    dice: "1d10",
    options: methods,
    result: naming.name ? { id: naming.method?.id ?? "custom", name: naming.name } : undefined,
    outcome: generated
      ? { chosen: false, dice: "1d10", roll: "8", rerolls: 0, follow_ups: [] }
      : undefined,
    naming,
    ...extra,
  };
}

const ordon = { id: "naming.kurovia.ordon", name: "Ordon" };
const founder = { id: "founder", name: "Historical Founder" };

function session(
  mode: SessionView["mode"],
  name: StageView,
  extra: Partial<SessionView> = {},
): SessionView {
  return {
    mode,
    god_mode: false,
    cursor: 2,
    complete: false,
    next_pending: 3,
    stages: [tableStage(0, "Setting Profile", "Kurovia"), tableStage(1, "Region", "Ordon"), name],
    suggested_name: name.naming?.name,
    ...extra,
  };
}

const ok = (v: SessionView): GenerationResponse => ({ view: v, error: null });

let settings: Settings;

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "0.2.0" })),
    getRoadmap: vi.fn(async () => []),
    getSettings: vi.fn(async () => settings),
    saveSettings: vi.fn(async (_p: Settings, s: Settings) => s),
    exitApp: vi.fn(),
    checkForUpdate: vi.fn(async () => null),
    generation: {
      start: vi.fn(),
      roll: vi.fn(),
      choose: vi.fn(),
      customName: vi.fn(),
      rollNamePart: vi.fn(),
      chooseNamePart: vi.fn(),
      advance: vi.fn(),
      back: vi.fn(),
      rollRemaining: vi.fn(),
      runAutomatic: vi.fn(),
      rerollAll: vi.fn(),
      discard: vi.fn(async () => {}),
      save: vi.fn(async (name: string) => ({ id: "stl-1", name })),
    },
  };
});

import { App } from "./App";
import { generation } from "./api";

const g = vi.mocked(generation);

async function openStart(first: SessionView) {
  g.start.mockResolvedValueOnce(ok(first));
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.2.0");
  await user.keyboard("{Enter}");
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  settings = {
    generation_method: "manual",
    generation_presentation: "incremental",
    god_mode: false,
    tag_view: false,
    theme: "modern_dark",
    bare_minimum_buildings: false,
  };
});

afterEach(cleanup);

describe("incremental name stage", () => {
  it("rolls a name, shows how it was made, and rerolls only the name", async () => {
    const pending = nameStage({ scheme: ordon });
    const user = await openStart(session("incremental", pending));
    expect(await screen.findByRole("heading", { name: "Name" })).toBeTruthy();
    expect(screen.getByText("Not named yet.")).toBeTruthy();

    g.roll.mockResolvedValueOnce(
      ok(
        session(
          "incremental",
          nameStage({ scheme: ordon, name: "Godwinsbury", source: "generated", method: founder }),
        ),
      ),
    );
    await user.click(screen.getByRole("button", { name: /Roll name/ }));
    expect(g.roll).toHaveBeenCalledWith(2);
    expect(await screen.findByText("Godwinsbury", { selector: ".name-value" })).toBeTruthy();
    expect(screen.getByText("Ordon naming · Historical Founder")).toBeTruthy();
    expect(screen.getByText(/Rolled/).textContent).toContain("1d10");

    // Rerolling targets the name stage only.
    g.roll.mockResolvedValueOnce(
      ok(
        session(
          "incremental",
          nameStage({
            scheme: ordon,
            name: "Alderwick",
            source: "generated",
            method: methods[0].result,
          }),
        ),
      ),
    );
    await user.click(screen.getByRole("button", { name: /Reroll name/ }));
    expect(g.roll).toHaveBeenLastCalledWith(2);
    expect(await screen.findByText("Alderwick", { selector: ".name-value" })).toBeTruthy();
  });

  it("chooses a naming method", async () => {
    const user = await openStart(session("incremental", nameStage({ scheme: ordon })));
    g.choose.mockResolvedValueOnce(
      ok(
        session(
          "incremental",
          nameStage({ scheme: ordon, name: "Bericwick", source: "generated", method: founder }),
        ),
      ),
    );
    const list = await screen.findByRole("group", { name: "Name results" });
    await user.click(within(list).getByRole("button", { name: /Historical Founder/ }));
    expect(g.choose).toHaveBeenCalledWith(2, "founder");
    expect(await screen.findByText("Bericwick", { selector: ".name-value" })).toBeTruthy();
  });

  it("accepts a typed name, checked before sending", async () => {
    const user = await openStart(session("incremental", nameStage({ scheme: ordon })));
    const input = await screen.findByLabelText("Or type a name");
    const use = screen.getByRole("button", { name: "Use name" }) as HTMLButtonElement;
    await user.type(input, "   ");
    expect(use.disabled).toBe(true); // nothing to use yet
    await user.type(input, "x".repeat(81));
    await user.click(use);
    expect(screen.getByRole("alert").textContent).toBe("Keep the name to 80 characters or fewer.");
    expect(g.customName).not.toHaveBeenCalled();

    g.customName.mockResolvedValueOnce(
      ok(session("incremental", nameStage({ name: "Saltmarsh", source: "custom" }))),
    );
    await user.clear(input);
    await user.type(input, "Saltmarsh");
    await user.click(screen.getByRole("button", { name: "Use name" }));
    expect(g.customName).toHaveBeenCalledWith(2, "Saltmarsh");
    expect(await screen.findByText("Typed name")).toBeTruthy();
  });

  it("explains a region without naming data and still allows a typed name", async () => {
    const skipped = nameStage(
      {},
      {
        status: "skipped",
        skipped: "no_naming_scheme",
        can_roll: false,
        options: [],
        dice: undefined,
      },
    );
    await openStart(session("incremental", skipped));
    expect(await screen.findByText(/no naming data for this region yet/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Roll name/ })).toBeNull();
    expect(screen.getByLabelText("Or type a name")).toBeTruthy();
  });

  it("shows routing and every roll only with Tag View", async () => {
    settings.tag_view = true;
    const detailed = nameStage({
      scheme: ordon,
      name: "Godwinsbury",
      source: "generated",
      method: founder,
      routing: [
        { id: "world.kurovia", name: "Kurovia" },
        { id: "region.ordon", name: "Ordon" },
      ],
      parts: [
        {
          table: { id: "f", name: "Ordon Name: Founder" },
          result: { id: "godwin", name: "Godwin" },
          dice: "1d100",
          roll: "74",
        },
        {
          table: { id: "e", name: "Ordon Name: Founder Ending" },
          result: { id: "bury", name: "bury" },
          dice: "1d12",
          roll: "6",
        },
      ],
    });
    await openStart(session("incremental", detailed));
    expect(await screen.findByText(/chosen by Kurovia \+ Ordon/)).toBeTruthy();
    expect(screen.getByText("Ordon Name: Founder")).toBeTruthy();
    expect(screen.getByText(/rolled 74 on 1d100/)).toBeTruthy();
  });
});

describe("one-table name row", () => {
  it("rolls, chooses a method and takes a typed name from the grid", async () => {
    settings.generation_presentation = "one_table";
    const user = await openStart(session("one_table", nameStage({ scheme: ordon })));
    const row = (await screen.findByText("Name", { selector: ".stage-row-name" })).closest("tr")!;
    g.roll.mockResolvedValueOnce(
      ok(
        session(
          "one_table",
          nameStage({
            scheme: ordon,
            name: "Alderwick",
            source: "generated",
            method: methods[0].result,
          }),
        ),
      ),
    );
    await user.click(within(row).getByRole("button", { name: /Roll/ }));
    expect(g.roll).toHaveBeenCalledWith(2);

    const updated = (await screen.findByText("Alderwick")).closest("tr")!;
    g.choose.mockResolvedValueOnce(ok(session("one_table", nameStage({ scheme: ordon }))));
    await user.selectOptions(within(updated).getByLabelText("Naming method"), "founder");
    expect(g.choose).toHaveBeenCalledWith(2, "founder");

    const current = (await screen.findByText("Name", { selector: ".stage-row-name" })).closest(
      "tr",
    )!;
    g.customName.mockResolvedValueOnce(
      ok(session("one_table", nameStage({ name: "Riverside", source: "custom" }))),
    );
    await user.type(within(current).getByLabelText("Type a name for Name"), "Riverside");
    await user.click(within(current).getByRole("button", { name: "Use name" }));
    expect(g.customName).toHaveBeenCalledWith(2, "Riverside");
  });
});

describe("saving", () => {
  it("offers the generated name when saving, which can still be changed", async () => {
    settings.generation_method = "automatic";
    const named = nameStage({
      scheme: ordon,
      name: "Godwinsbury",
      source: "generated",
      method: founder,
    });
    const done = session("automatic", named, {
      complete: true,
      next_pending: undefined,
      cursor: 3,
    });
    g.runAutomatic.mockResolvedValueOnce(ok(done));
    const user = await openStart(done);
    expect((await screen.findAllByText("Godwinsbury")).length).toBeGreaterThan(0); // in the sheet
    await user.click(screen.getByRole("button", { name: "Save" }));
    const input = screen.getByLabelText("Settlement name") as HTMLInputElement;
    expect(input.value).toBe("Godwinsbury");
    await user.clear(input);
    await user.type(input, "Godwin's Rest");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(g.save).toHaveBeenCalledWith("Godwin's Rest");
  });
});

describe("name parts and approved variants", () => {
  const curated = founder;
  const parts = (name: string, form: "a" | "b"): NamingView => ({
    scheme: ordon,
    name,
    source: "generated",
    method: curated,
    part_options: [
      {
        part: 0,
        id: "part1",
        label: "Ordon Manorial: Historical Person or Landholder",
        table: { id: "table.naming.ordon.founder", name: "Founder" },
        options: [
          { result: { id: "oswald", name: "Oswald" }, tags: [], legal: true, range: 28 },
          { result: { id: "godwin", name: "Godwin" }, tags: [], legal: true, range: 74 },
        ],
        previews: [name, form === "a" ? "Godwinsbury" : "Godwinham"],
        current: "oswald",
      },
      {
        part: 1,
        id: "part2",
        label: "Approved form",
        table: { id: "table.naming.ordon.founderEnding", name: "Approved form" },
        options: [
          { result: { id: "a", name: "A" }, tags: [], legal: true, range: [1, 2] },
          { result: { id: "b", name: "B" }, tags: [], legal: true, range: [3, 4] },
        ],
        previews: ["Oswalden", "Oswaldwick"],
        current: form,
      },
    ],
  });

  it("shows each variant's approved name and chooses or rolls one part", async () => {
    const user = await openStart(session("incremental", nameStage(parts("Oswaldwick", "b"))));
    const form = (await screen.findByLabelText("Approved form")) as HTMLSelectElement;
    expect(form.value).toBe("b");
    const labels = within(form)
      .getAllByRole("option")
      .map((o) => o.textContent);
    expect(labels).toContain("A → Oswalden (1–2)");
    expect(labels).toContain("B → Oswaldwick (3–4)");

    g.chooseNamePart.mockResolvedValueOnce(
      ok(session("incremental", nameStage(parts("Oswalden", "a")))),
    );
    await user.selectOptions(form, "a");
    expect(g.chooseNamePart).toHaveBeenCalledWith(2, 1, "a");
    expect(await screen.findByText("Oswalden", { selector: ".name-value" })).toBeTruthy();

    g.rollNamePart.mockResolvedValueOnce(
      ok(session("incremental", nameStage(parts("Oswaldwick", "b")))),
    );
    await user.click(screen.getByRole("button", { name: "Roll Approved form" }));
    expect(g.rollNamePart).toHaveBeenCalledWith(2, 1);
    expect(await screen.findByText("Oswaldwick", { selector: ".name-value" })).toBeTruthy();
  });

  it("offers part controls in the one-table grid too", async () => {
    settings.generation_presentation = "one_table";
    const user = await openStart(session("one_table", nameStage(parts("Oswaldwick", "b"))));
    const founderSelect = await screen.findByLabelText(
      "Ordon Manorial: Historical Person or Landholder",
    );
    g.chooseNamePart.mockResolvedValueOnce(
      ok(session("one_table", nameStage(parts("Godwinham", "b")))),
    );
    await user.selectOptions(founderSelect, "godwin");
    expect(g.chooseNamePart).toHaveBeenCalledWith(2, 0, "godwin");
  });

  it("shows stable IDs, chosen parts and the assembly with Tag View", async () => {
    settings.tag_view = true;
    const detailed = nameStage({
      ...parts("Oswaldwick", "b"),
      routing: [
        { id: "world.kurovia", name: "Kurovia" },
        { id: "region.ordon", name: "Ordon" },
      ],
      parts: [
        {
          table: { id: "table.naming.ordon.founder", name: "Ordon Manorial" },
          result: { id: "oswald", name: "Oswald" },
          dice: "1d100",
          roll: "28",
        },
        {
          table: { id: "table.naming.ordon.founderEnding", name: "Approved form table" },
          result: { id: "b", name: "B" },
          chosen: true,
        },
      ],
      assembly: {
        join: "curated_variant",
        curated: {
          table: "table.naming.ordon.founder",
          source: "oswald",
          field: "approved_names",
          variant: "b",
        },
      },
    });
    await openStart(session("incremental", detailed));
    expect(await screen.findByText(/rolled 28 on 1d100/)).toBeTruthy();
    expect(screen.getByText("oswald")).toBeTruthy();
    expect(screen.getByText(/— chosen$/)).toBeTruthy();
    expect(
      screen.getByText("Approved name of oswald (table.naming.ordon.founder) chosen by variant b"),
    ).toBeTruthy();
  });
});
