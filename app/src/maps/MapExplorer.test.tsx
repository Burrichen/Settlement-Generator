// The Map Explorer: zoom, pan, selection by geometry, keyboard use, districts
// with their own views, empty states, rendering cost, and the Location
// Inspector (lazy building details, editing, regeneration). The core is
// mocked at the `api` boundary, with a small in-memory building store.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BuildingDetailsView, BuildingEdits, SettlementMapsView } from "./types";
import {
  SETTLEMENT,
  districtMap,
  fakeDetails,
  grid,
  makeMap,
  plannedDistricts,
  plannedSettlement,
  view,
} from "./testMaps";

let current: SettlementMapsView;
/** Stored building details, as the core keeps them: generated once. */
let stored: Map<string, BuildingDetailsView>;

function editsOfStored(d: BuildingDetailsView): BuildingEdits {
  const edits: BuildingEdits = {};
  for (const key of [
    "name",
    "building_type",
    "condition",
    "occupancy",
    "occupants",
    "description",
  ] as const) {
    const f = d[key];
    if (f.origin === "user") Object.assign(edits, { [key]: { value: f.value ?? null } });
  }
  if (d.notes) edits.notes = d.notes;
  return edits;
}

function change(id: string, version: number) {
  const d = stored.get(id);
  if (!d) throw "This building's details have not been generated yet.";
  if (d.version !== version) throw "These details were changed elsewhere after they were opened.";
  return d;
}

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    maps: {
      get: vi.fn(async () => current),
      generate: vi.fn(async () => current),
      remove: vi.fn(),
      saveView: vi.fn(async () => undefined),
      building: vi.fn(async (_s: string, mapId: string, id: string) => {
        if (!stored.has(id)) stored.set(id, fakeDetails(mapId, id));
        return stored.get(id)!;
      }),
      editBuilding: vi.fn(
        async (_s: string, mapId: string, id: string, version: number, edits: BuildingEdits) => {
          const d = change(id, version);
          const next = fakeDetails(mapId, id, edits, version + 1, d.generation);
          stored.set(id, next);
          return next;
        },
      ),
      regenerateBuilding: vi.fn(
        async (_s: string, mapId: string, id: string, version: number, discard: boolean) => {
          const d = change(id, version);
          const edits = editsOfStored(d);
          const kept = discard ? (d.notes ? { notes: d.notes } : {}) : edits;
          const next = fakeDetails(mapId, id, kept, version + 1, d.generation + 1);
          stored.set(id, next);
          return next;
        },
      ),
      resetBuilding: vi.fn(async (_s: string, mapId: string, id: string, version: number) => {
        const d = change(id, version);
        const next = fakeDetails(mapId, id, d.notes ? { notes: d.notes } : {}, version + 1);
        stored.set(id, next);
        return next;
      }),
    },
  };
});

import { maps } from "../api";
import { MapExplorer } from "./MapExplorer";
import { layerBuildCount } from "./paths";

const api = vi.mocked(maps);

/** A Town map with 9 buildings on a 12 m grid around the origin. */
function townMap(count = 9) {
  return makeMap("map-settlement", "Testford", grid("map-settlement", count));
}

async function open(v: SettlementMapsView) {
  current = v;
  render(<MapExplorer settlementId={SETTLEMENT} settlementName="Testford" />);
  return screen.findByRole("application", { name: /Map of/ }).catch(() => null);
}

function canvas() {
  return screen.getByRole("application", { name: /Map of/ }) as unknown as SVGSVGElement;
}

function viewportOf() {
  const svg = canvas();
  return {
    k: Number(svg.getAttribute("data-zoom")),
    tx: Number(svg.getAttribute("data-tx")),
    ty: Number(svg.getAttribute("data-ty")),
  };
}

/** Screen position of a map point under the current view. */
function onScreen([x, y]: [number, number]) {
  const v = viewportOf();
  return { clientX: x * v.k + v.tx, clientY: y * v.k + v.ty };
}

function click(point: { clientX: number; clientY: number }) {
  fireEvent.mouseDown(canvas(), { button: 0, ...point });
  fireEvent.mouseUp(window, point);
}

beforeEach(() => {
  vi.clearAllMocks();
  stored = new Map();
});

afterEach(cleanup);

describe("empty states", () => {
  it("offers Generate Map and never generates on its own", async () => {
    await open(view([], plannedSettlement(false)));
    expect(await screen.findByText("No map yet")).toBeTruthy();
    expect(api.generate).not.toHaveBeenCalled();

    current = view([townMap()], plannedSettlement(true));
    await userEvent.click(screen.getByRole("button", { name: "Generate map" }));
    expect(api.generate).toHaveBeenCalledWith(SETTLEMENT, {});
    expect(await screen.findByRole("application", { name: "Map of Testford" })).toBeTruthy();
  });

  it("explains when a settlement cannot be mapped, without a Generate button", async () => {
    await open(
      view([], plannedSettlement(false), {
        problem: "This City has no districts recorded yet.",
        planned: [],
      }),
    );
    expect(await screen.findByText(/no districts recorded yet/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Generate/ })).toBeNull();
  });
});

describe("zoom and pan", () => {
  it("zooms with the wheel about the pointer", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const before = viewportOf();
    const pointer = { clientX: 300, clientY: 200 };
    const under = [
      (pointer.clientX - before.tx) / before.k,
      (pointer.clientY - before.ty) / before.k,
    ];
    fireEvent.wheel(canvas(), { deltaY: -100, ...pointer });
    const after = viewportOf();
    expect(after.k).toBeGreaterThan(before.k);
    expect(under[0] * after.k + after.tx).toBeCloseTo(pointer.clientX);
    expect(under[1] * after.k + after.ty).toBeCloseTo(pointer.clientY);
    fireEvent.wheel(canvas(), { deltaY: 100, ...pointer });
    expect(viewportOf().k).toBeCloseTo(before.k);
  });

  it("pans by dragging, and a drag does not select", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const before = viewportOf();
    const start = onScreen([0, 0]);
    fireEvent.mouseDown(canvas(), { button: 0, ...start });
    fireEvent.mouseMove(window, { clientX: start.clientX + 30, clientY: start.clientY + 10 });
    fireEvent.mouseMove(window, { clientX: start.clientX + 60, clientY: start.clientY + 30 });
    fireEvent.mouseUp(window, { clientX: start.clientX + 60, clientY: start.clientY + 30 });
    const after = viewportOf();
    expect(after.tx - before.tx).toBeCloseTo(60);
    expect(after.ty - before.ty).toBeCloseTo(30);
    expect(screen.queryByTestId("map-selection")).toBeNull();
  });

  it("zoom buttons, Fit map and Reset view", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const home = viewportOf();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(viewportOf().k).toBeGreaterThan(home.k);
    await user.click(screen.getByRole("button", { name: "Fit map" }));
    expect(viewportOf().k).toBeLessThan(home.k); // the whole frame is larger than the built area
    await user.click(screen.getByRole("button", { name: "Reset view" }));
    expect(viewportOf()).toEqual(home);
  });

  it("works from the keyboard", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    canvas().focus();
    const home = viewportOf();
    await user.keyboard("{ArrowLeft}");
    expect(viewportOf().tx).toBeCloseTo(home.tx + 60);
    await user.keyboard("+");
    expect(viewportOf().k).toBeGreaterThan(home.k);
    await user.keyboard("{Home}");
    expect(viewportOf()).toEqual(home);
  });
});

describe("selection", () => {
  it("selects the building under a click by its footprint and shows it in the inspector", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const builds = layerBuildCount();
    const target = current.maps!.maps[0].geometry.buildings[4]; // the centre of the 3×3 grid
    click(onScreen(target.centroid));
    const inspector = screen.getByRole("region", { name: "Location Inspector" });
    expect(within(inspector).getAllByText("Building 5").length).toBeGreaterThan(0);
    expect(await within(inspector).findByRole("heading", { name: "No. 5, Road 1" })).toBeTruthy();
    expect(screen.getByTestId("map-selection")).toBeTruthy();
    // Selection never regenerates or rebuilds the map.
    expect(api.generate).not.toHaveBeenCalled();
    expect(layerBuildCount()).toBe(builds);

    // Clicking between buildings clears it.
    click(onScreen([target.centroid[0] + 6, target.centroid[1]]));
    expect(screen.queryByTestId("map-selection")).toBeNull();
  });

  it("highlights the building under the pointer", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const target = current.maps!.maps[0].geometry.buildings[0];
    expect(document.querySelector(".map-hover")).toBeNull();
    fireEvent.mouseMove(canvas(), onScreen(target.centroid));
    expect(document.querySelector(".map-hover")).not.toBeNull();
    fireEvent.mouseLeave(canvas());
    expect(document.querySelector(".map-hover")).toBeNull();
  });

  it("Escape clears the selection without leaving the screen", async () => {
    const seen: boolean[] = [];
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && seen.push(e.defaultPrevented);
    window.addEventListener("keydown", onKey);
    try {
      await open(view([townMap()], plannedSettlement(true)));
      click(onScreen(current.maps!.maps[0].geometry.buildings[2].centroid));
      expect(screen.getByTestId("map-selection")).toBeTruthy();
      canvas().focus();
      await userEvent.keyboard("{Escape}");
      expect(screen.queryByTestId("map-selection")).toBeNull();
      // With nothing selected, Escape goes back as everywhere else.
      await userEvent.keyboard("{Escape}");
      expect(seen).toEqual([true, false]);
    } finally {
      window.removeEventListener("keydown", onKey);
    }
  });

  it("the building directory selects without a pointer", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Building directory" }));
    await user.type(screen.getByRole("searchbox", { name: "Search buildings" }), "Building 7");
    const list = screen.getByRole("listbox", { name: "Buildings" });
    expect(within(list).getAllByRole("option")).toHaveLength(1);
    within(list).getByRole("option").focus();
    await user.keyboard("{Enter}");
    const inspector = screen.getByRole("region", { name: "Location Inspector" });
    expect(await within(inspector).findByRole("heading", { name: "No. 7, Road 1" })).toBeTruthy();
    expect(screen.getByTestId("map-selection")).toBeTruthy();
  });
});

describe("panels", () => {
  it("the legend shows this map's features and can be searched and collapsed", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    const legend = screen.getByRole("region", { name: "Legend" });
    const key = within(legend).getByRole("list", { name: "Map key" });
    expect(within(key).getByText("Road")).toBeTruthy();
    expect(within(key).getByText("River")).toBeTruthy();
    expect(within(key).queryByText("Field")).toBeNull(); // this map has none
    await user.type(within(legend).getByRole("searchbox"), "wall");
    expect(
      within(key)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["Wall and towers"]);
    const toggle = within(legend).getByRole("button", { name: "Legend" });
    await user.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
  });

  it("the side panel can be hidden to give the map more room", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    await userEvent.click(screen.getByRole("button", { name: "Side panel" }));
    expect(screen.queryByRole("complementary", { name: "Map details" })).toBeNull();
  });
});

describe("districts", () => {
  it("lists the settlement's districts, generates one alone and keeps each view", async () => {
    await open(view([districtMap(1)], plannedDistricts([true, false, false])));
    const user = userEvent.setup();
    const tabs = screen.getByRole("tablist", { name: "Districts" });
    expect(
      within(tabs)
        .getAllByRole("tab")
        .map((t) => t.textContent),
    ).toEqual(["District 1", "District 2No map", "District 3No map"]);

    // Zoom District 1, then visit District 2.
    fireEvent.wheel(canvas(), { deltaY: -100, clientX: 200, clientY: 150 });
    const zoomed = viewportOf();
    await user.click(within(tabs).getByRole("tab", { name: /District 2/ }));
    expect(screen.getByText("District 2 has no map yet")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Generate all 2 missing districts" })).toBeTruthy();

    current = view([districtMap(1), districtMap(2)], plannedDistricts([true, true, false]));
    await user.click(screen.getByRole("button", { name: "Generate District 2 map" }));
    expect(api.generate).toHaveBeenCalledWith(SETTLEMENT, { only: "map-district-2" });
    expect(await screen.findByRole("application", { name: "Map of District 2" })).toBeTruthy();

    // Back to District 1: its zoom and position are as they were left.
    await user.click(within(tabs).getByRole("tab", { name: "District 1" }));
    expect(await screen.findByRole("application", { name: "Map of District 1" })).toBeTruthy();
    expect(viewportOf()).toEqual(zoomed);
    expect(api.generate).toHaveBeenCalledTimes(1);
  });

  it("switches districts with the arrow keys", async () => {
    await open(view([districtMap(1), districtMap(2)], plannedDistricts([true, true])));
    const user = userEvent.setup();
    screen.getByRole("tab", { name: "District 1" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "District 2" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    expect(await screen.findByRole("application", { name: "Map of District 2" })).toBeTruthy();
  });

  it("keeps a selection per district", async () => {
    await open(view([districtMap(1), districtMap(2)], plannedDistricts([true, true])));
    const user = userEvent.setup();
    click(onScreen(current.maps!.maps[0].geometry.buildings[0].centroid));
    expect(screen.getByTestId("map-selection")).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: "District 2" }));
    await screen.findByRole("application", { name: "Map of District 2" });
    expect(screen.queryByTestId("map-selection")).toBeNull();
    await user.click(screen.getByRole("tab", { name: "District 1" }));
    await screen.findByRole("application", { name: "Map of District 1" });
    expect(screen.getByTestId("map-selection")).toBeTruthy();
  });

  it("offers to redraw a map drawn before the settlement changed", async () => {
    const planned = plannedDistricts([true]);
    planned[0].stale = true;
    await open(view([districtMap(1)], planned, { status: "stale" }));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Redraw map" }));
    await user.click(screen.getByRole("button", { name: "Redraw" }));
    await waitFor(() =>
      expect(api.generate).toHaveBeenCalledWith(SETTLEMENT, { only: "map-district-1" }),
    );
  });
});

describe("performance", () => {
  it("draws 600 buildings with a handful of elements, and selection redraws none of them", async () => {
    await open(view([townMap(600)], plannedSettlement(true)));
    const svg = canvas();
    const elements = svg.querySelectorAll("*").length;
    expect(elements).toBeLessThan(30);
    expect(screen.getByTestId("map-buildings").getAttribute("d")!.match(/M/g)).toHaveLength(600);

    const staticLayer = svg.querySelector(".map-static");
    const builds = layerBuildCount();
    const t0 = performance.now();
    for (const b of current.maps!.maps[0].geometry.buildings.slice(0, 50)) {
      click(onScreen(b.centroid));
    }
    for (let i = 0; i < 20; i++) fireEvent.wheel(canvas(), { deltaY: i % 2 ? 100 : -100 });
    const elapsed = performance.now() - t0;
    expect(svg.querySelector(".map-static")).toBe(staticLayer); // never re-mounted
    expect(layerBuildCount()).toBe(builds);
    expect(elapsed).toBeLessThan(2000);
    // Only the buildings actually selected get details; arriving details
    // never redraw the map either.
    await waitFor(() => expect(stored.size).toBe(50));
    expect(layerBuildCount()).toBe(builds);
  });
});

// ----- Location Inspector ----------------------------------------------------------

function inspector() {
  return screen.getByRole("region", { name: "Location Inspector" });
}

/** Selects building `n` (0-based) of the active map and waits for its details. */
async function selectBuilding(n: number) {
  click(onScreen(current.maps!.maps[0].geometry.buildings[n].centroid));
  await within(inspector()).findByText("Generated", { selector: ".map-state" });
}

describe("location inspector", () => {
  it("generates a building's details on first selection only, and never on hover", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const target = current.maps!.maps[0].geometry.buildings[0];
    fireEvent.mouseMove(canvas(), onScreen(target.centroid));
    expect(api.building).not.toHaveBeenCalled();

    click(onScreen(target.centroid));
    expect(within(inspector()).getByText("Generating details…")).toBeTruthy();
    expect(await within(inspector()).findByText("House")).toBeTruthy();
    expect(within(inspector()).getByText("Generated", { selector: ".map-state" })).toBeTruthy();
    expect(within(inspector()).getByText("Ivy climbs one wall.")).toBeTruthy();
    expect(api.building).toHaveBeenCalledTimes(1);
    expect(api.building).toHaveBeenCalledWith(SETTLEMENT, "map-settlement", target.id);

    // Selecting another building and coming back reads the cache.
    await selectBuilding(1);
    click(onScreen(target.centroid));
    expect(within(inspector()).getByRole("heading", { name: "No. 1, Road 1" })).toBeTruthy();
    expect(api.building).toHaveBeenCalledTimes(2);
    expect(api.generate).not.toHaveBeenCalled();
  });

  it("shows where each value came from, and the record behind it", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    await selectBuilding(0);
    const panel = inspector();
    expect(within(panel).getAllByText("Rolled").length).toBeGreaterThan(3);
    expect(within(panel).getByText("Inferred")).toBeTruthy();
    expect(within(panel).getByText("Sources and record")).toBeTruthy();
    expect(within(panel).getByText(/Rolled 1d10 → 4 on Building Type: House/)).toBeTruthy();
    expect(within(panel).getByText("bld-settlement-r1-0001")).toBeTruthy();
    // No Notable Location stands here yet; one can be placed.
    const make = within(panel).getByRole("button", { name: "Make Notable Location" });
    expect((make as HTMLButtonElement).disabled).toBe(false);
    expect(within(panel).getByText("None")).toBeTruthy();
  });

  it("edits details in one save, marks them customised, and can undo", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    const form = within(inspector()).getByRole("form", { name: /Edit details of Building 1/ });
    const name = within(form).getByLabelText("Name or address");
    await user.clear(name);
    await user.type(name, "The Crooked Lantern");
    await user.type(within(form).getByLabelText("Notes"), "Owes the guild.");
    // Typing writes nothing; saving writes once.
    expect(api.editBuilding).not.toHaveBeenCalled();
    await user.click(within(form).getByRole("button", { name: "Save changes" }));

    expect(api.editBuilding).toHaveBeenCalledTimes(1);
    expect(api.editBuilding).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      1,
      { name: { value: "The Crooked Lantern" }, notes: "Owes the guild." },
    );
    const panel = inspector();
    expect(await within(panel).findByText("Customised", { selector: ".map-state" })).toBeTruthy();
    expect(within(panel).getByRole("heading", { name: "The Crooked Lantern" })).toBeTruthy();
    expect(within(panel).getByText("Edited")).toBeTruthy();
    expect(within(panel).getByText("Owes the guild.")).toBeTruthy();

    // The directory lists it by its new name.
    await user.click(screen.getByRole("button", { name: "Building directory" }));
    const listed = screen.getAllByRole("option").map((o) => o.textContent);
    expect(listed[0]).toBe("The Crooked Lantern (Building 1)House · on Road 1");

    // Undo puts the previous values back (from the new version).
    await user.click(within(panel).getByRole("button", { name: "Undo" }));
    await within(panel).findByText("Generated", { selector: ".map-state" });
    expect(api.editBuilding).toHaveBeenLastCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      2,
      {},
    );
  });

  it("keeps blank fields blank, refuses bad numbers and can give back a generated value", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    const form = within(inspector()).getByRole("form");
    const occupants = within(form).getByLabelText("Occupants");
    const save = within(form).getByRole("button", { name: "Save changes" });

    await user.clear(occupants);
    await user.type(occupants, "lots");
    expect(within(form).getByText(/whole number from 0 to 10000/)).toBeTruthy();
    expect((save as HTMLButtonElement).disabled).toBe(true);

    // "Use generated" restores the rolled value and drops the edit.
    await user.click(within(form).getAllByRole("button", { name: "Use generated" })[0]);
    expect((occupants as HTMLInputElement).value).toBe("3");
    expect((save as HTMLButtonElement).disabled).toBe(true); // nothing changed

    await user.clear(occupants);
    await user.click(save);
    expect(api.editBuilding).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      1,
      { occupants: { value: null } },
    );
    expect(await within(inspector()).findByText("Left blank")).toBeTruthy();
  });

  it("cancels with Escape, asking first about unsaved changes, without clearing the selection", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    // No changes: Escape just closes the form.
    await user.keyboard("{Escape}");
    expect(within(inspector()).queryByRole("form")).toBeNull();
    expect(screen.getByTestId("map-selection")).toBeTruthy();

    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    await user.type(within(inspector()).getByLabelText("Condition"), " but damp");
    await user.keyboard("{Escape}");
    const ask = within(inspector()).getByRole("alertdialog", { name: "Discard unsaved changes" });
    await user.click(within(ask).getByRole("button", { name: "Keep editing" }));
    expect((within(inspector()).getByLabelText("Condition") as HTMLInputElement).value).toBe(
      "Sound but damp",
    );
    await user.click(within(inspector()).getByRole("button", { name: "Cancel" }));
    await user.click(within(inspector()).getByRole("button", { name: "Discard changes" }));
    expect(within(inspector()).queryByRole("form")).toBeNull();
    expect(within(inspector()).getByText("Sound")).toBeTruthy();
    expect(api.editBuilding).not.toHaveBeenCalled();
    expect(screen.getByTestId("map-selection")).toBeTruthy();
  });

  it("keeps an unsaved draft while another building is looked at", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    await user.type(within(inspector()).getByLabelText("Notes"), "Half-written");
    await selectBuilding(1);
    expect(within(inspector()).queryByRole("form")).toBeNull();
    click(onScreen(current.maps!.maps[0].geometry.buildings[0].centroid));
    expect((within(inspector()).getByLabelText("Notes") as HTMLTextAreaElement).value).toBe(
      "Half-written",
    );
  });

  it("regenerates only after confirmation, keeping the user's values unless told otherwise", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    // Reset has nothing to do yet.
    const reset = within(inspector()).getByRole("button", { name: "Reset…" });
    expect((reset as HTMLButtonElement).disabled).toBe(true);

    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    const condition = within(inspector()).getByLabelText("Condition");
    await user.clear(condition);
    await user.type(condition, "Haunted");
    await user.click(within(inspector()).getByRole("button", { name: "Save changes" }));
    await within(inspector()).findByText("Customised", { selector: ".map-state" });

    await user.click(within(inspector()).getByRole("button", { name: "Regenerate…" }));
    const ask = within(inspector()).getByRole("alertdialog", { name: "Regenerate details" });
    expect(within(ask).getByText(/Fields you changed, and your notes, are kept/)).toBeTruthy();
    expect(api.regenerateBuilding).not.toHaveBeenCalled();
    await user.click(within(ask).getByRole("button", { name: "Regenerate" }));
    expect(api.regenerateBuilding).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      2,
      false,
    );
    expect(await within(inspector()).findByText("Workshop")).toBeTruthy();
    expect(within(inspector()).getByText("Haunted")).toBeTruthy();

    // Replacing the user's values needs the extra, explicit choice.
    await user.click(within(inspector()).getByRole("button", { name: "Regenerate…" }));
    const again = within(inspector()).getByRole("alertdialog", { name: "Regenerate details" });
    await user.click(
      within(again).getByRole("checkbox", { name: "Also replace my changes (condition)" }),
    );
    expect(within(again).getByText(/will be lost/)).toBeTruthy();
    await user.click(
      within(again).getByRole("button", { name: "Regenerate and replace my changes" }),
    );
    expect(api.regenerateBuilding).toHaveBeenLastCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      3,
      true,
    );
    expect(
      await within(inspector()).findByText("Generated", { selector: ".map-state" }),
    ).toBeTruthy();
    expect(within(inspector()).queryByText("Haunted")).toBeNull();
  });

  it("resets changed fields to their generated values after confirmation", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Edit details" }));
    const type = within(inspector()).getByLabelText("Type");
    await user.clear(type);
    await user.type(type, "Bakery");
    await user.click(within(inspector()).getByRole("button", { name: "Save changes" }));
    await within(inspector()).findByText("Bakery");

    await user.click(within(inspector()).getByRole("button", { name: "Reset…" }));
    const ask = within(inspector()).getByRole("alertdialog", { name: "Reset generated details" });
    // The safe choice has the focus.
    expect(document.activeElement).toBe(within(ask).getByRole("button", { name: "Cancel" }));
    await user.click(within(ask).getByRole("button", { name: "Reset to generated" }));
    expect(api.resetBuilding).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-settlement",
      "bld-settlement-r1-0001",
      2,
    );
    expect(await within(inspector()).findByText("House")).toBeTruthy();
  });

  it("reports a change made elsewhere and reloads the details", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    await selectBuilding(0);
    // The other interface edits the building meanwhile.
    stored.set(
      "bld-settlement-r1-0001",
      fakeDetails("map-settlement", "bld-settlement-r1-0001", { notes: "From the terminal" }, 2),
    );
    await user.click(within(inspector()).getByRole("button", { name: "Regenerate…" }));
    await user.click(within(inspector()).getByRole("button", { name: "Regenerate" }));
    expect(await screen.findByText(/changed elsewhere/)).toBeTruthy();
    expect(await within(inspector()).findByText("From the terminal")).toBeTruthy();
  });

  it("offers to try again when details cannot be loaded", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    api.building.mockRejectedValueOnce("The generator content is invalid.");
    click(onScreen(current.maps!.maps[0].geometry.buildings[0].centroid));
    expect(await within(inspector()).findByText("The generator content is invalid.")).toBeTruthy();
    await user.click(within(inspector()).getByRole("button", { name: "Try again" }));
    expect(await within(inspector()).findByText("House")).toBeTruthy();
  });

  it("opening a building in one district generates nothing in the others", async () => {
    await open(view([districtMap(1), districtMap(2)], plannedDistricts([true, true])));
    const user = userEvent.setup();
    await selectBuilding(2);
    await user.click(screen.getByRole("tab", { name: "District 2" }));
    await screen.findByRole("application", { name: "Map of District 2" });
    expect(api.building).toHaveBeenCalledTimes(1);
    expect(api.building).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-district-1",
      "bld-district-1-r1-0003",
    );
    expect([...stored.keys()]).toEqual(["bld-district-1-r1-0003"]);
  });

  it("Locate on map centres the view on the building", async () => {
    await open(view([townMap()], plannedSettlement(true)));
    const user = userEvent.setup();
    const target = current.maps!.maps[0].geometry.buildings[8];
    await selectBuilding(8);
    await user.click(within(inspector()).getByRole("button", { name: "Locate on map" }));
    const v = viewportOf();
    expect(target.centroid[0] * v.k + v.tx).toBeCloseTo(400, 0);
    expect(target.centroid[1] * v.k + v.ty).toBeCloseTo(260, 0);
  });
});
