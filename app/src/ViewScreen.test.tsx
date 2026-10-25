// Saved settlement view: list navigation, actions, favourite rules and the
// edit workflow. The core is mocked at the `api` boundary.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GenerationResponse, SavedSettlementView, SettlementSummary, Settings } from "./api";

function summary(id: string, name: string, favourite = false): SettlementSummary {
  return {
    id,
    name,
    favourite,
    created_at: "2026-10-11T10:00:00Z",
    updated_at: "2026-10-11T10:00:00Z",
    app_version: "0.1.0",
  };
}

let items: SettlementSummary[];
let settings: Settings;

function detailOf(s: SettlementSummary): SavedSettlementView {
  return {
    ...s,
    stages: [
      {
        label: "Setting Profile",
        stage_table: "table.settingProfile",
        table: { id: "table.settingProfile", name: "Setting Profile" },
        result: { id: "kurovia", name: "Kurovia" },
        tags: [{ id: "world.kurovia", name: "Kurovia tag" }],
      },
      {
        label: "Region",
        stage_table: "table.genericRegion",
        table: { id: "table.kuroviaRegion", name: "Kurovia Region" },
        result: { id: "ordon", name: `Ordon of ${s.name}` },
      },
      {
        label: "Economy",
        stage_table: "table.economy",
        table: { id: "table.economy", name: "Economy" },
        result: { id: "prosperous", name: "Prosperous" },
        tags: [{ id: "economy.prosperous", name: "Prosperous tag" }],
      },
      {
        label: "Authority",
        stage_table: "table.authority",
        table: { id: "table.authority", name: "Authority" },
        result: { id: "council", name: "Local Council" },
        tags: [{ id: "authority.council", name: "Local Council tag" }],
      },
      {
        label: "Representation",
        stage_table: "table.authorityRepresentation",
        skipped: true,
        sub_stage: true,
      },
      {
        label: "Authority Size",
        stage_table: "table.authoritySize",
        table: { id: "table.authoritySize", name: "Authority Size" },
        result: { id: "four", name: "4 Representatives" },
        sub_stage: true,
      },
    ],
  };
}

const editView: GenerationResponse = {
  error: null,
  view: {
    mode: "one_table",
    god_mode: false,
    cursor: 2,
    complete: true,
    stages: [
      {
        index: 0,
        stage: { id: "table.settingProfile", name: "Setting Profile" },
        table: { id: "table.settingProfile", name: "Setting Profile" },
        status: "resolved",
        current: false,
        actionable: true,
        can_roll: false,
        options: [{ result: { id: "kurovia", name: "Kurovia" }, tags: [], legal: true }],
        result: { id: "kurovia", name: "Kurovia" },
      },
    ],
  },
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
    generation: { ...actual.generation, discard: vi.fn(async () => {}), choose: vi.fn() },
    saved: {
      list: vi.fn(async () => items),
      get: vi.fn(async (id: string) => detailOf(items.find((s) => s.id === id)!)),
      setFavourite: vi.fn(async (id: string, favourite: boolean) => {
        items = items.map((s) => (s.id === id ? { ...s, favourite } : s));
        return detailOf(items.find((s) => s.id === id)!);
      }),
      remove: vi.fn(async (id: string) => {
        items = items.filter((s) => s.id !== id);
      }),
      editStart: vi.fn(async () => editView),
      editSave: vi.fn(async (name: string) => {
        items = items.map((s) => (s.id === "a" ? { ...s, name } : s));
        return detailOf(items[0]);
      }),
      exportSettlement: vi.fn(async (id: string) => `C:/Exports/${id}.settlement.json`),
      importSettlement: vi.fn(async (text: string) => {
        if (!text.includes("settlement-generator.settlement")) {
          throw "This is not a Settlement Generator settlement export.";
        }
        const added = summary("z", "Ærøskøbing – Ōshū 城");
        items = [...items, added];
        return added;
      }),
    },
    maps: {
      get: vi.fn(async (id: string) => ({
        settlement_id: id,
        status: "not_generated",
        layout: "single",
        engine: { id: "builtin.medieval", version: "1.0.0", features: [] },
        planned: [
          {
            id: "map-settlement",
            label: "x",
            scope: { kind: "settlement" },
            generated: false,
            stale: false,
          },
        ],
      })),
      generate: vi.fn(),
      remove: vi.fn(),
    },
  };
});

import { App } from "./App";
import { generation, maps, saved } from "./api";

const api = vi.mocked(saved);

async function openView() {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.1.0");
  await user.keyboard("{ArrowDown}{ArrowDown}{Enter}"); // Start, Settings, View
  await screen.findByRole("listbox", { name: "Saved settlements" });
  await screen.findByRole("article", { name: /snapshot/ });
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  items = [summary("a", "Ashford"), summary("b", "Brightwater", true), summary("c", "Coldmere")];
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

describe("list", () => {
  it("lists settlements with a star before favourites", async () => {
    await openView();
    const options = screen.getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["Ashford", "★Brightwater", "Coldmere"]);
    expect(options[1].className).toContain("is-favourite");
    expect(options[0].getAttribute("aria-selected")).toBe("true");
  });

  it("moves the selection with Up/Down and shows the selected snapshot", async () => {
    const user = await openView();
    screen.getAllByRole("option")[0].focus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    const options = screen.getAllByRole("option");
    expect(options[2].getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(options[2]);
    expect(await screen.findByText("Ordon of Coldmere")).toBeTruthy();
    await user.keyboard("{ArrowUp}");
    expect(await screen.findByText("Ordon of Brightwater")).toBeTruthy();
  });

  it("selects with a mouse click", async () => {
    const user = await openView();
    await user.click(screen.getByText("Coldmere"));
    expect(await screen.findByText("Ordon of Coldmere")).toBeTruthy();
  });

  it("shows an empty state when nothing is saved", async () => {
    items = [];
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("v0.1.0");
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(await screen.findByText(/No saved settlements yet/)).toBeTruthy();
  });
});

describe("snapshot pane", () => {
  it("shows every stage, the generated-with version, and no tags without Tag View", async () => {
    await openView();
    const sheet = screen.getByRole("article", { name: /snapshot/ });
    expect(within(sheet).getByText("Kurovia")).toBeTruthy();
    expect(within(sheet).getByText("Kurovia Region")).toBeTruthy();
    expect(within(sheet).getByText("Economy")).toBeTruthy();
    expect(within(sheet).getByText("Prosperous")).toBeTruthy();
    expect(screen.getByText(/Generated with v0\.1\.0/)).toBeTruthy();
    expect(within(sheet).queryByText("Kurovia tag")).toBeNull();
    expect(within(sheet).queryByText("Prosperous tag")).toBeNull();
    expect(within(sheet).getByText("Authority")).toBeTruthy();
    expect(within(sheet).getByText("Local Council")).toBeTruthy();
    // The Authority sub-stages are folded into the Authority row.
    const authority = within(sheet).getByText("Authority").closest(".sheet-row") as HTMLElement;
    expect(within(authority).getByText("4 Representatives")).toBeTruthy();
    expect(within(sheet).queryByText("Representation")).toBeNull();
    expect(within(sheet).queryByText("Not applicable")).toBeNull();
    expect(within(sheet).queryByText("Local Council tag")).toBeNull();
  });

  it("shows tags with Tag View", async () => {
    settings.tag_view = true;
    await openView();
    expect(screen.getByText("Kurovia tag")).toBeTruthy();
    expect(screen.getByText("Prosperous tag")).toBeTruthy();
    expect(screen.getByText("Local Council tag")).toBeTruthy();
  });
});

describe("actions", () => {
  const actionNames = () =>
    within(screen.getByRole("toolbar", { name: "Settlement actions" }))
      .getAllByRole("button")
      .map((b) => b.textContent);

  it("offers Edit, Favourite and Delete for a non-favourite", async () => {
    await openView();
    expect(actionNames()).toEqual(["Edit", "Export", "Favourite", "Delete"]);
  });

  it("never offers Delete for a favourite", async () => {
    const user = await openView();
    await user.click(screen.getByText("Brightwater"));
    await screen.findByText("Ordon of Brightwater");
    expect(actionNames()).toEqual(["Edit", "Export", "Unfavourite"]);
  });

  it("Right moves into the actions and Left returns to the list", async () => {
    const user = await openView();
    screen.getAllByRole("option")[0].focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement?.textContent).toBe("Edit");
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement?.textContent).toBe("Export");
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement?.textContent).toBe("Favourite");
    await user.keyboard("{ArrowLeft}{ArrowLeft}{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getAllByRole("option")[0]);
  });

  it("favourites and unfavourites through the core", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Favourite" }));
    expect(api.setFavourite).toHaveBeenCalledWith("a", true);
    expect(await screen.findByRole("button", { name: "Unfavourite" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Unfavourite" }));
    expect(api.setFavourite).toHaveBeenLastCalledWith("a", false);
  });

  it("deletes only after confirmation and selects a neighbour", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(api.remove).not.toHaveBeenCalled();
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }),
    );
    expect(api.remove).toHaveBeenCalledWith("a");
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(2));
    expect(await screen.findByText("Ordon of Brightwater")).toBeTruthy();
  });
});

describe("edit", () => {
  it("opens the snapshot for editing and saves an explicit revision", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(api.editStart).toHaveBeenCalledWith("a");
    const nameInput = await screen.findByLabelText("Name");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true); // nothing changed yet

    await user.clear(nameInput);
    await user.click(save);
    expect(screen.getByText("Enter a name for the settlement.")).toBeTruthy();
    expect(api.editSave).not.toHaveBeenCalled();

    await user.type(nameInput, "Ashford Revisited");
    await user.click(save);
    expect(api.editSave).toHaveBeenCalledWith("Ashford Revisited");
    expect(await screen.findByRole("toolbar", { name: "Settlement actions" })).toBeTruthy();
    expect(screen.getAllByRole("option")[0].textContent).toBe("Ashford Revisited");
  });

  it("cancels with the button or Escape, discarding the edit and staying on View", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await screen.findByLabelText("Name");
    await user.keyboard("{Escape}");
    expect(generation.discard).toHaveBeenCalled();
    expect(await screen.findByRole("toolbar", { name: "Settlement actions" })).toBeTruthy();
    expect(screen.getByRole("listbox")).toBeTruthy();
    expect(api.editSave).not.toHaveBeenCalled();
  });

  it("asks before discarding an edit with changes", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const nameInput = await screen.findByLabelText("Name");
    await user.type(nameInput, " Revisited");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    const confirm = screen.getByRole("alertdialog");
    expect(generation.discard).not.toHaveBeenCalled();
    await user.click(within(confirm).getByRole("button", { name: "Keep it" }));
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("Ashford Revisited");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Discard" }),
    );
    expect(generation.discard).toHaveBeenCalled();
    expect(await screen.findByRole("toolbar", { name: "Settlement actions" })).toBeTruthy();
  });

  it("locks the list while editing, so another settlement cannot be picked", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await screen.findByLabelText("Name");
    expect(
      screen.getByRole("navigation", { name: "Saved settlements" }).hasAttribute("inert"),
    ).toBe(true);
  });
});

describe("map tab", () => {
  it("opens the selected settlement's map without generating one", async () => {
    const user = await openView();
    const tabs = screen.getByRole("tablist", { name: "Settlement view" });
    expect(
      within(tabs).getByRole("tab", { name: "Settlement" }).getAttribute("aria-selected"),
    ).toBe("true");
    await user.click(within(tabs).getByRole("tab", { name: "Map" }));
    expect(await screen.findByText("No map yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Generate map" })).toBeTruthy();
    expect(maps.get).toHaveBeenCalledWith("a");
    expect(maps.generate).not.toHaveBeenCalled();

    // The tab stays open when another settlement is chosen.
    await user.click(screen.getByText("Coldmere"));
    await waitFor(() => expect(maps.get).toHaveBeenCalledWith("c"));

    // Left/Right move between tabs.
    within(tabs).getByRole("tab", { name: "Map" }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(await screen.findByRole("article", { name: /snapshot/ })).toBeTruthy();
  });

  it("Escape on the map tab with nothing selected still goes back", async () => {
    const user = await openView();
    await user.click(screen.getByRole("tab", { name: "Map" }));
    await screen.findByText("No map yet");
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });
});

describe("settlement export and import", () => {
  it("exports the selected settlement with its maps and says where", async () => {
    const user = await openView();
    await user.click(screen.getByRole("button", { name: "Export" }));
    expect(api.exportSettlement).toHaveBeenCalledWith("a");
    expect(await screen.findByRole("status")).toHaveProperty(
      "textContent",
      "Exported with all its maps to C:/Exports/a.settlement.json",
    );
  });

  it("imports an export file as a new settlement and selects it", async () => {
    const user = await openView();
    const text = JSON.stringify({ format: "settlement-generator.settlement" });
    const file = new File([text], "x.settlement.json", { type: "application/json" });
    await user.upload(screen.getByLabelText("Settlement export file"), file);
    expect(api.importSettlement).toHaveBeenCalledWith(text);
    expect(
      await screen.findByText(/Imported “Ærøskøbing – Ōshū 城” as a new settlement/),
    ).toBeTruthy();
    const selected = screen.getByRole("option", { selected: true });
    expect(selected.textContent).toContain("Ærøskøbing – Ōshū 城");

    // A file that is not an export is refused with the reason.
    await user.upload(
      screen.getByLabelText("Settlement export file"),
      new File(["nonsense"], "notes.json", { type: "application/json" }),
    );
    expect(await screen.findByText(/not a Settlement Generator settlement export/)).toBeTruthy();
    expect(screen.getAllByRole("option")).toHaveLength(4);
  });
});
