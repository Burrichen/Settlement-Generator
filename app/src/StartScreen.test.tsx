// Generation flows in the GUI. The core is replaced by scripted responses at
// the `api` boundary; the real generation rules are tested in Rust.
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AuthorityFigures,
  FiguresView,
  GenerationResponse,
  NotableLocations,
  SessionView,
  Settings,
  StageView,
} from "./api";

const STAGES = [
  "Setting Profile",
  "Region",
  "Environment",
  "Connections",
  "Size",
  "Origin",
  "Purpose",
  "Age",
  "Economy",
  "Authority",
  "Security",
  "Crime",
  "Stability",
];
const RESULTS = [
  "Kurovia",
  "Ordon",
  "Coast",
  "Major Route",
  "Town",
  "Trade Stop",
  "Fishing",
  "Old",
  "Prosperous",
  "Local Council",
  "Guarded",
  "Moderate",
  "Cohesive",
];

function stage(index: number, resolved: boolean, extra: Partial<StageView> = {}): StageView {
  const name = STAGES[index];
  return {
    index,
    stage: { id: `table.${index}`, name },
    table: { id: `table.${index}`, name },
    status: resolved ? "resolved" : "pending",
    current: false,
    actionable: true,
    can_roll: index !== 0,
    dice: index === 0 ? undefined : "1d20",
    options: [
      { result: { id: `r${index}`, name: RESULTS[index] }, tags: [], legal: true, range: [1, 10] },
      {
        result: { id: `x${index}`, name: `Other ${index}` },
        tags: [],
        legal: true,
        range: [11, 20],
      },
    ],
    result: resolved ? { id: `r${index}`, name: RESULTS[index] } : undefined,
    outcome: resolved
      ? { chosen: index === 0, dice: "1d20", roll: "7", rerolls: 0, follow_ups: [] }
      : undefined,
    tags: resolved && tagView ? [{ id: `tag.${index}`, name: `Tag ${index}` }] : undefined,
    ...extra,
  };
}

function view(
  mode: SessionView["mode"],
  resolvedCount: number,
  extra: Partial<SessionView> = {},
): SessionView {
  return {
    mode,
    god_mode: false,
    cursor: 0,
    complete: resolvedCount === STAGES.length,
    next_pending: resolvedCount < STAGES.length ? resolvedCount : undefined,
    stages: STAGES.map((_, i) => stage(i, i < resolvedCount)),
    active_tags: tagView ? [{ id: "world.kurovia", name: "Kurovia" }] : undefined,
    ...extra,
  };
}

const ok = (v: SessionView): GenerationResponse => ({ view: v, error: null });

let tagView = false;
let settings: Settings;

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "0.1.0" })),
    getRoadmap: vi.fn(async () => []),
    getSettings: vi.fn(async () => settings),
    saveSettings: vi.fn(async (_previous: Settings, s: Settings) => s),
    exitApp: vi.fn(),
    generation: {
      start: vi.fn(),
      view: vi.fn(),
      roll: vi.fn(),
      choose: vi.fn(),
      clear: vi.fn(),
      advance: vi.fn(),
      back: vi.fn(),
      rollRemaining: vi.fn(),
      runAutomatic: vi.fn(),
      rerollAll: vi.fn(),
      chooseFigureField: vi.fn(),
      chooseRole: vi.fn(),
      rollDistrictCount: vi.fn(),
      chooseDistrictCount: vi.fn(),
      rollLocationCount: vi.fn(),
      chooseLocationCount: vi.fn(),
      rollLocationType: vi.fn(),
      chooseLocationType: vi.fn(),
      rollLocationDetail: vi.fn(),
      chooseLocationDetail: vi.fn(),
      chooseFigureNamePart: vi.fn(),
      figureName: vi.fn(),
      chooseRelationship: vi.fn(),
      rollFigureName: vi.fn(),
      discard: vi.fn(async () => {}),
      save: vi.fn(async (name: string) => ({ id: "stl-1", name })),
    },
  };
});

import { App } from "./App";
import { generation } from "./api";

const g = vi.mocked(generation);

async function openStart() {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.1.0");
  await user.keyboard("{Enter}"); // Start is the first menu item
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  tagView = false;
  settings = {
    generation_method: "automatic",
    generation_presentation: "incremental",
    god_mode: false,
    tag_view: false,
    theme: "modern_dark",
    bare_minimum_buildings: false,
  };
});

afterEach(cleanup);

describe("automatic", () => {
  beforeEach(() => {
    g.start.mockResolvedValue(ok(view("automatic", 0)));
  });

  it("asks only for the Setting Profile, then shows the completed settlement", async () => {
    g.runAutomatic
      .mockResolvedValueOnce(ok(view("automatic", 0)))
      .mockResolvedValueOnce(ok(view("automatic", STAGES.length)));
    g.choose.mockResolvedValue(ok(view("automatic", 1)));
    const user = await openStart();

    expect(await screen.findByRole("heading", { name: "Choose a Setting Profile" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: /Kurovia/ }));
    expect(g.choose).toHaveBeenCalledWith(0, "r0");

    const sheet = await screen.findByRole("article", { name: "Generated settlement" });
    for (const [i, name] of STAGES.entries()) {
      expect(within(sheet).getByText(name)).toBeTruthy();
      expect(within(sheet).getByText(RESULTS[i])).toBeTruthy();
    }
    expect(within(sheet).getByText("Not saved yet")).toBeTruthy();
    // No tag clutter without Tag View.
    expect(within(sheet).queryByText(/Tags/)).toBeNull();
  });

  it("stops with a readable message where content is incomplete", async () => {
    const blocked = view("automatic", 2);
    blocked.stages[2] = stage(2, false, {
      can_roll: false,
      incomplete: "Environment for other regions has not been designed.",
    });
    g.runAutomatic.mockResolvedValueOnce({
      view: blocked,
      error: "Environment cannot be generated yet: its content is incomplete",
    });
    await openStart();
    expect(await screen.findByRole("heading", { name: "Environment needs a choice" })).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain(
      "Environment cannot be generated yet: its content is incomplete",
    );
  });
});

describe("completed settlement actions", () => {
  async function completed() {
    g.start.mockResolvedValue(ok(view("automatic", STAGES.length)));
    g.runAutomatic.mockResolvedValue(ok(view("automatic", STAGES.length)));
    const user = await openStart();
    await screen.findByRole("article", { name: "Generated settlement" });
    return user;
  }

  it("has a Map tab that explains maps need a saved settlement", async () => {
    const user = await completed();
    await user.click(screen.getByRole("tab", { name: "Map" }));
    expect(screen.getByText(/Maps belong to saved settlements/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Generate/ })).toBeNull();
    await user.click(screen.getByRole("tab", { name: "Settlement" }));
    expect(screen.getByRole("article", { name: "Generated settlement" })).toBeTruthy();
  });

  it("re-rolls the entire settlement", async () => {
    g.rerollAll.mockResolvedValue(ok(view("automatic", STAGES.length)));
    const user = await completed();
    await user.click(screen.getByRole("button", { name: "Re-roll" }));
    expect(g.rerollAll).toHaveBeenCalledTimes(1);
    expect(g.roll).not.toHaveBeenCalled();
  });

  it("saves under a validated name and returns to the Main Menu", async () => {
    const user = await completed();
    await user.click(screen.getByRole("button", { name: "Save" }));
    const input = screen.getByLabelText("Settlement name");
    expect(document.activeElement).toBe(input);

    await user.click(within(screen.getByRole("form")).getByRole("button", { name: "Save" }));
    expect(screen.getByText("Enter a name for the settlement.")).toBeTruthy();
    await user.type(input, "   ");
    await user.keyboard("{Enter}");
    expect(g.save).not.toHaveBeenCalled();

    await user.type(input, "Riverside  {Enter}");
    expect(g.save).toHaveBeenCalledWith("Riverside");
    expect(await screen.findByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });

  it("rejects overly long names", async () => {
    const user = await completed();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.type(screen.getByLabelText("Settlement name"), `${"x".repeat(81)}{Enter}`);
    expect(screen.getByText(/80 characters or fewer/)).toBeTruthy();
    expect(g.save).not.toHaveBeenCalled();
  });

  it("deletes only after confirmation, then returns to the Main Menu", async () => {
    const user = await completed();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(g.discard).not.toHaveBeenCalled();
    // Focus starts on the safe choice.
    expect(document.activeElement?.textContent).toBe("Keep it");
    await user.click(screen.getByRole("button", { name: "Keep it" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }),
    );
    expect(g.discard).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });

  it("shows tags as secondary text when Tag View is on", async () => {
    tagView = true;
    settings.tag_view = true;
    await completed();
    const sheet = screen.getByRole("article", { name: "Generated settlement" });
    expect(within(sheet).getByText("Tag 3")).toBeTruthy();
  });
});

describe("manual incremental", () => {
  beforeEach(() => {
    settings.generation_method = "manual";
  });

  it("presents one stage at a time and only advances once resolved", async () => {
    g.start.mockResolvedValue(ok(view("incremental", 0)));
    g.choose.mockResolvedValue(ok(view("incremental", 1)));
    g.advance.mockResolvedValue(ok(view("incremental", 1, { cursor: 1 })));
    g.roll.mockResolvedValue(ok(view("incremental", 2, { cursor: 1 })));
    const user = await openStart();

    expect(await screen.findByRole("heading", { name: "Setting Profile" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Region" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Roll/ })).toBeNull(); // chosen, not rolled
    const next = screen.getByRole("button", { name: /Next stage/ });
    expect((next as HTMLButtonElement).disabled).toBe(true);

    await user.click(screen.getByRole("button", { name: /Kurovia/ }));
    expect(g.choose).toHaveBeenCalledWith(0, "r0");
    await user.click(screen.getByRole("button", { name: /Next stage/ }));
    expect(await screen.findByRole("heading", { name: "Region" })).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /Roll 1d20/ }));
    expect(g.roll).toHaveBeenCalledWith(1);
    expect(await screen.findByText(/Rolled/)).toBeTruthy();
  });

  it("explains a stage invalidated by an earlier change", async () => {
    const v = view("incremental", 3, { cursor: 3 });
    v.stages[3] = stage(3, false, { previous: { id: "majorRoute", name: "Major Route" } });
    g.start.mockResolvedValue(ok(v));
    await openStart();
    expect(await screen.findByText(/An earlier change affects this stage/)).toBeTruthy();
    const note = screen.getByText(/An earlier change affects this stage/);
    expect(within(note).getByText("Major Route").tagName).toBe("S");
  });

  it("asks before leaving with Escape once something was generated", async () => {
    g.start.mockResolvedValue(ok(view("incremental", 2, { cursor: 2 })));
    const user = await openStart();
    await screen.findByRole("heading", { name: "Environment" });
    await user.keyboard("{Escape}");
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    expect(screen.queryByRole("navigation", { name: "Main menu" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Discard and leave" }));
    expect(g.discard).toHaveBeenCalled();
    expect(await screen.findByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });

  it("shows active and emitted tags only with Tag View", async () => {
    tagView = true;
    settings.tag_view = true;
    g.start.mockResolvedValue(ok(view("incremental", 1)));
    await openStart();
    expect(await screen.findByRole("complementary", { name: "Active tags" })).toBeTruthy();
    expect(screen.getByText("Tag 0")).toBeTruthy();
  });
});

describe("manual one-table", () => {
  beforeEach(() => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
  });

  it("shows every stage together and lets any stage be rolled or chosen", async () => {
    g.start.mockResolvedValue(ok(view("one_table", 0)));
    g.roll.mockResolvedValue(ok(view("one_table", 0)));
    g.choose.mockResolvedValue(ok(view("one_table", 1)));
    g.rollRemaining.mockResolvedValue(ok(view("one_table", STAGES.length)));
    const user = await openStart();

    await screen.findByRole("table");
    expect(screen.getAllByRole("combobox")).toHaveLength(STAGES.length);
    await user.click(screen.getByRole("button", { name: "Roll Purpose" }));
    expect(g.roll).toHaveBeenCalledWith(6);
    await user.click(screen.getByRole("button", { name: "Roll Economy" }));
    expect(g.roll).toHaveBeenCalledWith(STAGES.indexOf("Economy"));
    await user.click(screen.getByRole("button", { name: "Roll Authority" }));
    expect(g.roll).toHaveBeenCalledWith(STAGES.indexOf("Authority"));
    await user.selectOptions(screen.getByLabelText("Setting Profile"), "r0");
    expect(g.choose).toHaveBeenCalledWith(0, "r0");

    const finish = screen.getByRole("button", { name: "Finish" }) as HTMLButtonElement;
    expect(finish.disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: "Roll remaining stages" }));
    expect(finish.disabled).toBe(false);
    await user.click(finish);
    expect(await screen.findByRole("article", { name: "Generated settlement" })).toBeTruthy();
  });
});

describe("authority sub-stages", () => {
  /** Every stage resolved, plus Representation (skipped: decided by the
   * Authority) and Authority Size (rolled), both sub-stages. */
  function withSubStages(mode: SessionView["mode"]): SessionView {
    const v = view(mode, STAGES.length);
    const n = STAGES.length;
    const sub = (i: number, name: string, extra: Partial<StageView>): StageView => ({
      ...stage(i, false),
      index: i,
      stage: { id: `table.sub${i}`, name },
      table: { id: `table.sub${i}`, name },
      options: [],
      sub_stage: true,
      ...extra,
    });
    // Straight after Authority, before Security, Crime and Stability.
    v.stages.splice(
      STAGES.indexOf("Authority") + 1,
      0,
      sub(n, "Representation", { status: "skipped", skipped: "not_eligible", can_roll: false }),
      sub(n + 1, "Authority Size", {
        status: "resolved",
        result: { id: "four", name: "4 Representatives" },
        options: [
          { result: { id: "four", name: "4 Representatives" }, tags: [], legal: true },
          { result: { id: "five", name: "5 Representatives" }, tags: [], legal: true },
        ],
      }),
    );
    v.complete = true;
    v.next_pending = undefined;
    return v;
  }

  it("shows Authority Size under Authority and leaves out what does not apply", async () => {
    g.start.mockResolvedValue(ok(withSubStages("automatic")));
    g.runAutomatic.mockResolvedValue(ok(withSubStages("automatic")));
    await openStart();
    const sheet = await screen.findByRole("article", { name: "Generated settlement" });
    const authority = within(sheet).getByText("Authority").closest(".sheet-row") as HTMLElement;
    expect(within(authority).getByText("Local Council")).toBeTruthy();
    expect(within(authority).getByText("4 Representatives")).toBeTruthy();
    expect(within(sheet).queryByText("Representation")).toBeNull();
    expect(within(sheet).queryByText("Authority Size")).toBeNull();
    expect(within(sheet).queryByText("Not applicable")).toBeNull();
  });

  it("shows only sub-stages that apply in the one-table grid", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    g.start.mockResolvedValue(ok(withSubStages("one_table")));
    await openStart();
    await screen.findByRole("table");
    expect(screen.queryByLabelText("Representation")).toBeNull();
    const size = screen.getByLabelText("Authority Size") as HTMLSelectElement;
    expect(size.value).toBe("four");
    expect(size.closest("tr")?.className).toContain("sub-stage");
    expect(screen.getAllByRole("combobox")).toHaveLength(STAGES.length + 1);
  });
});

describe("authority figures", () => {
  const record: AuthorityFigures = {
    request: {
      authority: { id: "council", name: "Local Council" },
      representation: "group",
      size: 2,
    },
    app_version: "0.1.1",
    figures: [
      {
        id: "npc-a",
        label: "Mila Varen",
        role: {
          allocation: "lead",
          role: { id: "role.council.chair", name: "Council Chair" },
          label: "Council Chair",
        },
        name: { source: "custom", name: "Mila Varen" },
        app_version: "0.1.1",
        fields: [
          ["species", "human", "Human"],
          ["gender", "woman", "Woman"],
          ["age", "middleAged", "Middle-aged"],
          ["disposition", "reserved", "Reserved"],
          ["motive", "preserveIndependence", "Preserve Independence"],
          ["reputation", "respected", "Respected"],
          ["flaw", "rigid", "Rigid / Inflexible"],
        ].map(([field, id, name]) => ({
          field,
          roll: { table: { id: `t.${field}`, name: field }, result: { id, name } },
        })),
      },
      {
        id: "npc-b",
        label: "Figure 2",
        name: { source: "missing", reason: "No naming data for Dwarf (Man) yet." },
        app_version: "0.1.1",
        fields: [
          {
            field: "species",
            roll: {
              table: { id: "t.species", name: "Species" },
              result: { id: "dwarf", name: "Dwarf" },
            },
          },
        ],
      },
    ],
    relationships: [
      {
        a: "npc-a",
        b: "npc-b",
        roll: {
          table: { id: "t.rel", name: "Rel" },
          result: { id: "distrusts", name: "Distrusts" },
        },
      },
    ],
  };
  const fieldOptions: FiguresView["fields"] = [
    {
      field: "species",
      label: "Species",
      table: { id: "t.species", name: "Kurovia Authority Figure Species" },
      options: [
        { result: { id: "human", name: "Human" }, tags: [], legal: true },
        { result: { id: "dwarf", name: "Dwarf" }, tags: [], legal: true },
        { result: { id: "firbolg", name: "Firbolg" }, tags: [], legal: false },
      ],
    },
  ];

  function withFigures(mode: SessionView["mode"], generated: boolean): SessionView {
    const v = view(mode, STAGES.length);
    const roleSlot = (allocation: "lead" | "member", table: string, ids: [string, string][]) => ({
      allocation,
      table: { id: table, name: table },
      options: ids.map(([id, name]) => ({ result: { id, name }, tags: [], legal: true })),
    });
    // Roles: a sub-stage before the figures.
    v.stages.push({
      ...stage(STAGES.length, generated),
      index: STAGES.length,
      stage: { id: "stage.authorityRoles", name: "Roles" },
      table: { id: "stage.authorityRoles", name: "Roles" },
      options: [],
      can_roll: true,
      sub_stage: true,
      result: generated ? { id: "authorityRoles", name: "Council Chair, Councillor" } : undefined,
      roles: {
        record: generated
          ? {
              request: record.request,
              scheme: { id: "roles.council", name: "Local Council roles" },
              app_version: "0.1.1",
              roles: [
                {
                  allocation: "lead",
                  roll: {
                    table: { id: "t.lead", name: "Lead" },
                    result: { id: "role.council.chair", name: "Council Chair" },
                  },
                  initial: { id: "role.council.chair", name: "Council Chair" },
                  role: { id: "role.council.chair", name: "Council Chair" },
                },
                {
                  allocation: "member",
                  roll: {
                    table: { id: "t.member", name: "Member" },
                    result: { id: "role.council.treasurer", name: "Treasurer" },
                  },
                  initial: { id: "role.council.treasurer", name: "Treasurer" },
                  fallback: { id: "role.council.councillor", name: "Councillor" },
                  role: { id: "role.council.councillor", name: "Councillor" },
                },
              ],
            }
          : undefined,
        slots: [
          roleSlot("lead", "Local Council Lead Role", [
            ["role.council.chair", "Council Chair"],
            ["role.council.civicSpeaker", "Civic Speaker"],
          ]),
          roleSlot("member", "Local Council Member Role", [
            ["role.council.councillor", "Councillor"],
            ["role.council.treasurer", "Treasurer"],
          ]),
        ],
      },
    });
    const i = STAGES.length + 1;
    v.stages.push({
      ...stage(i, generated),
      index: i,
      stage: { id: "stage.authorityFigures", name: "Authority Figures" },
      table: { id: "stage.authorityFigures", name: "Authority Figures" },
      options: [],
      can_roll: true,
      result: generated ? { id: "authorityFigures", name: "2 Authority Figures" } : undefined,
      figures: {
        record: generated ? record : undefined,
        fields: fieldOptions,
        relationship_options: [
          { result: { id: "distrusts", name: "Distrusts" }, tags: [], legal: true },
          { result: { id: "admires", name: "Admires" }, tags: [], legal: true },
        ],
      },
    });
    v.complete = generated;
    return v;
  }

  it("shows a compact outline of each figure and their relationships", async () => {
    g.start.mockResolvedValue(ok(withFigures("automatic", true)));
    g.runAutomatic.mockResolvedValue(ok(withFigures("automatic", true)));
    await openStart();
    const sheet = await screen.findByRole("article", { name: "Generated settlement" });
    const row = within(sheet).getByText("Authority Figures").closest(".sheet-row") as HTMLElement;
    expect(within(row).getByText("Mila Varen")).toBeTruthy();
    expect(within(row).getByText("Human • Woman • Middle-aged")).toBeTruthy();
    expect(within(row).getByText("Disposition: Reserved")).toBeTruthy();
    expect(within(row).getByText("Motive: Preserve Independence")).toBeTruthy();
    expect(within(row).getByText("Reputation: Respected")).toBeTruthy();
    expect(within(row).getByText("Flaw: Rigid / Inflexible")).toBeTruthy();
    // A figure without naming data says so instead of inventing a name.
    expect(within(row).getByText("Figure 2")).toBeTruthy();
    expect(within(row).getByText("No naming data for Dwarf (Man) yet.")).toBeTruthy();
    expect(within(row).getByText("Mila Varen ↔ Figure 2 — Distrusts")).toBeTruthy();
  });

  it("lets each field, name and relationship be set in one-table mode", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    const v = withFigures("one_table", true);
    g.start.mockResolvedValue(ok(v));
    g.chooseFigureField.mockResolvedValue(ok(v));
    g.figureName.mockResolvedValue(ok(v));
    g.chooseRelationship.mockResolvedValue(ok(v));
    const user = await openStart();
    const i = STAGES.length + 1;

    const species = (await screen.findByLabelText("Mila Varen: Species")) as HTMLSelectElement;
    expect(species.value).toBe("human");
    // Only legal results are offered.
    expect(within(species).queryByText("Firbolg")).toBeNull();
    await user.selectOptions(species, "dwarf");
    expect(g.chooseFigureField).toHaveBeenCalledWith(i, 0, "species", "dwarf");

    await user.type(screen.getByLabelText("Figure 2: type a name"), "Orven Daal");
    await user.click(screen.getAllByRole("button", { name: "Set name" })[1]);
    expect(g.figureName).toHaveBeenCalledWith(i, 1, "Orven Daal");

    await user.selectOptions(
      screen.getByLabelText("Mila Varen ↔ Figure 2: relationship"),
      "admires",
    );
    expect(g.chooseRelationship).toHaveBeenCalledWith(i, 0, "admires");
  });

  it("offers only the figure's own species name pools", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    const v = withFigures("one_table", true);
    const figuresStage = v.stages[v.stages.length - 1];
    const roll = (table: string, id: string, name: string) => ({
      table: { id: table, name: table },
      result: { id, name },
    });
    figuresStage.figures!.record!.figures[0].name = {
      source: "generated",
      name: "Mila Varen",
      scheme: { id: "naming.person.human.feminine", name: "Human names (feminine)" },
      method: { id: "givenSurname", name: "Given name + family name" },
      parts: [
        { id: "given", label: "Given name", result: { id: "mila", name: "Mila" } },
        { id: "family", label: "Family name", result: { id: "varen", name: "Varen" } },
      ],
      rolls: [
        roll("table.personName.human.method", "givenSurname", "Given name + family name"),
        roll("table.personName.human.given.feminine", "mila", "Mila"),
        roll("table.personName.human.family", "varen", "Varen"),
      ],
    };
    figuresStage.figures!.name_parts = [
      [
        {
          part: 0,
          id: "given",
          label: "Given name",
          table: {
            id: "table.personName.human.given.feminine",
            name: "Human Feminine Given Names",
          },
          options: [
            { result: { id: "mila", name: "Mila" }, tags: [], legal: true },
            { result: { id: "adelis", name: "Adelis" }, tags: [], legal: true },
          ],
        },
      ],
      [],
    ];
    g.start.mockResolvedValue(ok(v));
    g.chooseFigureNamePart.mockResolvedValue(ok(v));
    const user = await openStart();
    const given = (await screen.findByLabelText("Mila Varen: Given name")) as HTMLSelectElement;
    expect(given.value).toBe("mila");
    expect(within(given).getAllByRole("option")).toHaveLength(2);
    await user.selectOptions(given, "adelis");
    expect(g.chooseFigureNamePart).toHaveBeenCalledWith(STAGES.length + 1, 0, 0, "adelis");
    // The unnamed figure has no name parts to choose.
    expect(screen.queryByLabelText("Figure 2: Given name")).toBeNull();
  });

  it("shows each figure's role above its name, not as a separate row", async () => {
    g.start.mockResolvedValue(ok(withFigures("automatic", true)));
    g.runAutomatic.mockResolvedValue(ok(withFigures("automatic", true)));
    await openStart();
    const sheet = await screen.findByRole("article", { name: "Generated settlement" });
    const role = within(sheet).getByText("Council Chair");
    expect(role.className).toBe("figure-role");
    expect(role.nextElementSibling?.textContent).toBe("Mila Varen");
    expect(within(sheet).queryByText("Roles")).toBeNull();
  });

  it("lets each role be chosen from its own table in one-table mode", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    const v = withFigures("one_table", true);
    g.start.mockResolvedValue(ok(v));
    g.chooseRole.mockResolvedValue(ok(v));
    const user = await openStart();
    const lead = (await screen.findByLabelText("Figure 1 · Lead: role")) as HTMLSelectElement;
    expect(lead.value).toBe("role.council.chair");
    // Only the lead table's results are offered for the lead.
    expect(within(lead).queryByText("Treasurer")).toBeNull();
    const member = screen.getByLabelText("Figure 2 · Member: role") as HTMLSelectElement;
    expect(member.value).toBe("role.council.councillor");
    expect(
      screen.getByText("Treasurer was already held, so this becomes Councillor."),
    ).toBeTruthy();
    await user.selectOptions(lead, "role.council.civicSpeaker");
    expect(g.chooseRole).toHaveBeenCalledWith(STAGES.length, 0, "role.council.civicSpeaker");
  });
});

describe("notable locations", () => {
  const named = (id: string, name: string) => ({ id, name });
  const roll = (table: string, id: string, name: string, faces: number[] = [1]) => ({
    table: named(table, table),
    dice: "1d6",
    attempts: faces.map((total) => ({ roll: { total } })),
    result: named(id, name),
  });
  const record: NotableLocations = {
    size: named("city", "City"),
    scheme: named("locations.city", "City Notable Locations"),
    districts: { roll: roll("City District Count", "two", "2 Districts"), count: 2 },
    areas: [
      {
        district: { id: "district-1", label: "District 1" },
        count: {
          roll: roll("City District Notable Location Count", "two", "2 Notable Locations"),
          count: 2,
        },
      },
      {
        district: { id: "district-2", label: "District 2" },
        count: {
          roll: roll("City District Notable Location Count", "two", "2 Notable Locations"),
          count: 2,
        },
      },
    ],
    locations: [
      {
        id: "notable-location-guaranteed-tavern",
        source: "bareMinimumBuildings",
        type: named("locationType.hospitality", "Hospitality"),
        subtype: named("locationSubtype.tavernInn", "Tavern / Inn"),
        spare_rooms: 4,
      },
      {
        id: "notable-location-01",
        source: "rolled",
        district: "district-1",
        type: named("locationType.hospitality", "Hospitality"),
        subtype: named("locationSubtype.eatingHouse", "Eating House"),
        subtype_rolls: {
          roll: roll("Hospitality", "locationSubtype.eatingHouse", "Eating House", [7]),
        },
      },
      {
        id: "notable-location-02",
        source: "rolled",
        district: "district-1",
        type: named("locationType.trade", "Trade / Shop"),
        subtype: named("locationSubtype.magicStore", "Magic Store"),
        subtype_rolls: {
          roll: roll("Trade / Shop", "locationSubtype.magicStore", "Magic Store", [15]),
        },
        speciality: named("magicStoreSpeciality.weapons", "Weapons"),
        speciality_roll: roll(
          "Magic Store Speciality",
          "magicStoreSpeciality.weapons",
          "Weapons",
          [5],
        ),
      },
      {
        id: "notable-location-03",
        source: "rolled",
        district: "district-2",
        type: named("locationType.criminal", "Criminal"),
      },
      {
        id: "notable-location-04",
        source: "rolled",
        district: "district-2",
        type: named("locationType.craft", "Craft / Workshop"),
        subtype: named("locationSubtype.armoursmith", "Armoursmith"),
        subtype_rolls: {
          roll: roll(
            "Craft / Workshop",
            "locationSubtype.specialistArtisan",
            "Specialist Artisan",
            [20],
          ),
          rerolls: [
            roll(
              "Craft / Workshop",
              "locationSubtype.specialistArtisan",
              "Specialist Artisan",
              [20],
            ),
            roll(
              "Craft / Workshop",
              "locationSubtype.weaponsmithArmoursmith",
              "Weaponsmith / Armoursmith",
              [6],
            ),
          ],
          split: {
            ...roll("Weaponsmith / Armoursmith", "locationSubtype.armoursmith", "Armoursmith", [3]),
            dice: "1d4",
          },
        },
        quality_shift: {
          steps: 1,
          source: named("locationSubtype.specialistArtisan", "Specialist Artisan"),
        },
      },
    ],
    issued: 4,
    app_version: "0.1.1",
  };
  const options = (table: string, ids: [string, string][]) => ({
    table: named(table, table),
    options: ids.map(([id, name]) => ({ result: named(id, name), tags: [], legal: true })),
  });
  const hospitality: [string, string][] = [
    ["locationSubtype.tavernInn", "Tavern / Inn"],
    ["locationSubtype.eatingHouse", "Eating House"],
    ["locationSubtype.bathhouse", "Bathhouse"],
  ];
  const trade: [string, string][] = [
    ["locationSubtype.generalSmithy", "General Smithy"],
    ["locationSubtype.magicStore", "Magic Store"],
  ];
  const specialities: [string, string][] = [
    ["magicStoreSpeciality.weapons", "Weapons"],
    ["magicStoreSpeciality.clothing", "Clothing"],
  ];
  const craft: [string, string][] = [
    ["locationSubtype.generalSmithy", "General Smithy"],
    ["locationSubtype.weaponsmithArmoursmith", "Weaponsmith / Armoursmith"],
    ["locationSubtype.bowyerFletcher", "Bowyer / Fletcher"],
    ["locationSubtype.specialistArtisan", "Specialist Artisan"],
  ];

  function withLocations(mode: SessionView["mode"]): SessionView {
    const v = view(mode, STAGES.length);
    const i = STAGES.length;
    v.stages.push({
      ...stage(i, true),
      index: i,
      stage: named("stage.notableLocations", "Notable Locations"),
      table: named("stage.notableLocations", "Notable Locations"),
      options: [],
      can_roll: true,
      result: named("notableLocations", "5 Notable Locations"),
      locations: {
        record,
        bare_minimum_buildings: true,
        options: {
          details: [
            {},
            { subtype: options("Hospitality", hospitality) },
            {
              subtype: options("Trade / Shop", trade),
              speciality: options("Magic Store Speciality", specialities),
            },
            {},
            {
              subtype: options("Craft / Workshop", craft),
              reroll: options(
                "Craft / Workshop",
                craft.filter(([id]) => id !== "locationSubtype.specialistArtisan"),
              ),
              split: options("Weaponsmith / Armoursmith", [
                ["locationSubtype.weaponsmith", "Weaponsmith"],
                ["locationSubtype.armoursmith", "Armoursmith"],
              ]),
            },
          ],
          districts: options("City District Count", [
            ["two", "2 Districts"],
            ["three", "3 Districts"],
          ]),
          count: options("City District Notable Location Count", [
            ["two", "2 Notable Locations"],
            ["three", "3 Notable Locations"],
          ]),
          types: options("Notable Location Type", [
            ["locationType.hospitality", "Hospitality"],
            ["locationType.trade", "Trade / Shop"],
            ["locationType.criminal", "Criminal"],
          ]),
        },
      },
    });
    return v;
  }

  it("lists the guaranteed Tavern first, then each rolled location with its district", async () => {
    g.start.mockResolvedValue(ok(withLocations("automatic")));
    g.runAutomatic.mockResolvedValue(ok(withLocations("automatic")));
    await openStart();
    const sheet = await screen.findByRole("article", { name: "Generated settlement" });
    const row = within(sheet).getByText("Notable Locations").closest(".sheet-row") as HTMLElement;
    const items = within(row)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    expect(items).toEqual([
      "Tavern / Inn · Hospitality · 4 spare rooms",
      "Eating House · Hospitality · District 1",
      "Magic Store (Weapons) · Trade / Shop · District 1",
      "Criminal · District 2",
      "Armoursmith · Craft / Workshop · Specialist Artisan (Quality +1 step) · District 2",
    ]);
    expect(within(row).getByText("2 Districts")).toBeTruthy();
  });

  it("lets counts and types be changed in one-table mode, but not the Tavern", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    const v = withLocations("one_table");
    g.start.mockResolvedValue(ok(v));
    g.chooseDistrictCount.mockResolvedValue(ok(v));
    g.chooseLocationCount.mockResolvedValue(ok(v));
    g.chooseLocationType.mockResolvedValue(ok(v));
    g.rollLocationType.mockResolvedValue(ok(v));
    const user = await openStart();
    const i = STAGES.length;

    await user.selectOptions(await screen.findByLabelText("District Count"), "three");
    expect(g.chooseDistrictCount).toHaveBeenCalledWith(i, "three");

    const second = screen.getByLabelText("District 2: Notable Location Count") as HTMLSelectElement;
    expect(second.value).toBe("two");
    await user.selectOptions(second, "three");
    expect(g.chooseLocationCount).toHaveBeenCalledWith(i, 1, "three");

    await user.selectOptions(screen.getByLabelText("Location 4: type"), "locationType.trade");
    expect(g.chooseLocationType).toHaveBeenCalledWith(i, 3, "locationType.trade");

    // The guaranteed Tavern / Inn has no type or subtype to choose.
    expect(screen.queryByLabelText("Location 1: type")).toBeNull();
    expect(screen.queryByLabelText("Location 1: subtype")).toBeNull();
    expect(screen.getByText("Added by Bare Minimum Buildings.")).toBeTruthy();
    expect(
      screen.getByText("Bare Minimum Buildings: one extra Tavern / Inn is added."),
    ).toBeTruthy();
  });

  it("offers each location's subtype, and only the child rolls that apply", async () => {
    settings.generation_method = "manual";
    settings.generation_presentation = "one_table";
    const v = withLocations("one_table");
    g.start.mockResolvedValue(ok(v));
    g.chooseLocationDetail.mockResolvedValue(ok(v));
    g.rollLocationDetail.mockResolvedValue(ok(v));
    const user = await openStart();
    const i = STAGES.length;

    // Hospitality: its subtype, nothing more.
    const eating = (await screen.findByLabelText("Location 2: subtype")) as HTMLSelectElement;
    expect(eating.value).toBe("locationSubtype.eatingHouse");
    expect(screen.queryByLabelText("Location 2: speciality")).toBeNull();
    await user.selectOptions(eating, "locationSubtype.bathhouse");
    expect(g.chooseLocationDetail).toHaveBeenCalledWith(
      i,
      1,
      "subtype",
      "locationSubtype.bathhouse",
    );

    // The Magic Store: its speciality too.
    const speciality = screen.getByLabelText("Location 3: speciality") as HTMLSelectElement;
    expect(speciality.value).toBe("magicStoreSpeciality.weapons");
    expect(screen.getByText("Location 3: Magic Store Speciality")).toBeTruthy();
    await user.selectOptions(speciality, "magicStoreSpeciality.clothing");
    expect(g.chooseLocationDetail).toHaveBeenCalledWith(
      i,
      2,
      "speciality",
      "magicStoreSpeciality.clothing",
    );
    expect(screen.queryByLabelText("Location 3: reroll")).toBeNull();

    // Criminal has no subtype table.
    expect(screen.queryByLabelText("Location 4: subtype")).toBeNull();

    // The Specialist Artisan: its reroll (never itself) and the d4 split,
    // with every roll shown.
    const artisan = screen.getByLabelText("Location 5: subtype") as HTMLSelectElement;
    expect(artisan.value).toBe("locationSubtype.specialistArtisan");
    expect(screen.getByText("Location 5: Specialist Artisan rolls again")).toBeTruthy();
    const reroll = screen.getByLabelText("Location 5: reroll") as HTMLSelectElement;
    expect(reroll.value).toBe("locationSubtype.weaponsmithArmoursmith");
    expect(within(reroll).queryByText("Specialist Artisan")).toBeNull();
    expect(screen.getByText("Craft / Workshop: 20 → 20 → 6.")).toBeTruthy();
    expect(screen.getByText("Quality +1 step, once Quality is generated.")).toBeTruthy();
    const split = screen.getByLabelText("Location 5: split") as HTMLSelectElement;
    expect(split.value).toBe("locationSubtype.armoursmith");
    expect(screen.getByText("1d4: 3.")).toBeTruthy();
    expect(screen.queryByLabelText("Location 5: speciality")).toBeNull();

    const row = reroll.closest("label") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Re-roll" }));
    expect(g.rollLocationDetail).toHaveBeenCalledWith(i, 4, "reroll");
  });
});
