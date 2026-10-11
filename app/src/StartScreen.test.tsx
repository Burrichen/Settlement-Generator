// Generation flows in the GUI. The core is replaced by scripted responses at
// the `api` boundary; the real generation rules are tested in Rust.
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GenerationResponse, SessionView, Settings, StageView } from "./api";

const STAGES = [
  "Setting Profile",
  "Region",
  "Environment",
  "Connections",
  "Size",
  "Origin",
  "Purpose",
  "Age",
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
      .mockResolvedValueOnce(ok(view("automatic", 8)));
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
    const blocked = view("automatic", 4);
    blocked.stages[4] = stage(4, false, {
      can_roll: false,
      incomplete: "The dice distribution for Size has not been finalised.",
    });
    g.runAutomatic.mockResolvedValueOnce({
      view: blocked,
      error: "Size cannot be generated yet: its content is incomplete",
    });
    await openStart();
    expect(await screen.findByRole("heading", { name: "Size needs a choice" })).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain(
      "Size cannot be generated yet: its content is incomplete",
    );
  });
});

describe("completed settlement actions", () => {
  async function completed() {
    g.start.mockResolvedValue(ok(view("automatic", 8)));
    g.runAutomatic.mockResolvedValue(ok(view("automatic", 8)));
    const user = await openStart();
    await screen.findByRole("article", { name: "Generated settlement" });
    return user;
  }

  it("re-rolls the entire settlement", async () => {
    g.rerollAll.mockResolvedValue(ok(view("automatic", 8)));
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
    g.rollRemaining.mockResolvedValue(ok(view("one_table", 8)));
    const user = await openStart();

    await screen.findByRole("table");
    expect(screen.getAllByRole("combobox")).toHaveLength(8);
    await user.click(screen.getByRole("button", { name: "Roll Purpose" }));
    expect(g.roll).toHaveBeenCalledWith(6);
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
