// District maps of Cities and Capitals: switching and searching districts,
// renaming one, deleting one district's map with a warning, points of
// interest, the settlement-wide directory, saved views and lazy loading.
// The core is mocked at the `api` boundary by the fake directory.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Point, SettlementMapsView } from "./types";
import {
  FakeDirectory,
  SETTLEMENT,
  district,
  districtMap,
  fakeDetails,
  location,
  plannedDistricts,
  view,
  withLocations,
} from "./testMaps";

let fake: FakeDirectory;

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    maps: {
      get: vi.fn(async () => fake.view),
      generate: vi.fn(async () => fake.view),
      remove: vi.fn(),
      building: vi.fn(async (_s: string, mapId: string, id: string) => fakeDetails(mapId, id)),
      assignLocation: vi.fn(),
      unassignLocation: vi.fn(),
      updateLocation: vi.fn(),
      createLocation: vi.fn(),
      deleteLocation: vi.fn(),
      deleteMap: vi.fn(async (_s: string, mapId: string) => fake.deleteMap(mapId)),
      renameDistrict: vi.fn(async (_s: string, d: string, name: string | null) =>
        fake.renameDistrict(d, name),
      ),
      addPoi: vi.fn(async (_s: string, mapId: string, p: Point, label: string, notes: string) =>
        fake.addPoi(mapId, p, label, notes),
      ),
      updatePoi: vi.fn(async (_s: string, id: string, label: string, notes: string) =>
        fake.updatePoi(id, label, notes),
      ),
      movePoi: vi.fn(async (_s: string, id: string, p: Point) => fake.movePoi(id, p)),
      deletePoi: vi.fn(async (_s: string, id: string) => fake.deletePoi(id)),
      saveView: vi.fn(async () => undefined),
    },
  };
});

import { maps } from "../api";
import { MapExplorer, VIEW_SAVE_DELAY } from "./MapExplorer";

const api = vi.mocked(maps);

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

/** A Capital with three districts, all drawn, and a location in each of two. */
function capital(): SettlementMapsView {
  const planned = plannedDistricts([true, true, true]);
  planned[1].annotations = { building_details: 4, locations: 1, pois: 2 };
  return withLocations(view([districtMap(1), districtMap(2), districtMap(3)], planned), [
    location(1, { district: district(1), name: "Old Hall" }),
    location(2, { district: district(2), name: "Dockside Inn" }),
  ]);
}

async function open(v: SettlementMapsView) {
  fake = new FakeDirectory(v);
  render(<MapExplorer settlementId={SETTLEMENT} settlementName="Capital" />);
  await screen.findByRole("application", { name: /Map of/ });
}

function canvas() {
  return screen.getByRole("application", { name: /Map of/ });
}

function viewportOf() {
  const svg = canvas();
  return {
    k: Number(svg.getAttribute("data-zoom")),
    tx: Number(svg.getAttribute("data-tx")),
    ty: Number(svg.getAttribute("data-ty")),
  };
}

function onScreen([x, y]: Point) {
  const v = viewportOf();
  return { clientX: x * v.k + v.tx, clientY: y * v.k + v.ty };
}

function clickAt(point: { clientX: number; clientY: number }) {
  fireEvent.mouseDown(canvas(), { button: 0, ...point });
  fireEvent.mouseUp(window, point);
}

function inspector() {
  return screen.getByRole("region", { name: "Location Inspector" });
}

function tab(name: string | RegExp) {
  return screen.getByRole("tab", { name });
}

describe("districts", () => {
  it("are searched and switched without drawing or generating anything", async () => {
    await open(capital());
    const user = userEvent.setup();
    await user.type(screen.getByRole("searchbox", { name: "Search districts" }), "3");
    await user.click(
      within(screen.getByRole("list", { name: "Matching districts" })).getByRole("button", {
        name: "District 3",
      }),
    );
    expect(tab("District 3").getAttribute("aria-selected")).toBe("true");
    expect(await screen.findByRole("application", { name: "Map of District 3" })).toBeTruthy();
    // Only the district on screen is drawn, and opening it generates nothing.
    expect(screen.getAllByRole("application")).toHaveLength(1);
    expect(api.generate).not.toHaveBeenCalled();
    expect(api.building).not.toHaveBeenCalled();
  });

  it("are renamed by name only", async () => {
    await open(capital());
    const user = userEvent.setup();
    await user.click(tab("District 2"));
    await user.click(screen.getByRole("button", { name: "Rename…" }));
    await user.type(screen.getByLabelText("District name"), "Harbour Ward");
    await user.click(screen.getByRole("button", { name: "Save name" }));
    expect(api.renameDistrict).toHaveBeenCalledWith(SETTLEMENT, "district-2", "Harbour Ward");
    expect(await screen.findByRole("tab", { name: "Harbour Ward" })).toBeTruthy();
    const title = document.querySelector(".map-district-title")!;
    expect(title.textContent).toBe("Harbour Ward · District 2");
    expect(screen.getByRole("application", { name: "Map of Harbour Ward" })).toBeTruthy();
    expect(screen.getByRole("tabpanel", { name: "Harbour Ward" })).toBeTruthy();
  });

  it("deleting a district's map warns what goes with it, and keeps the district", async () => {
    await open(capital());
    const user = userEvent.setup();
    await user.click(tab("District 2"));
    await user.click(screen.getByRole("button", { name: "Delete map…" }));
    const ask = screen.getByRole("alertdialog", { name: /Delete the map of District 2/ });
    expect(ask.getAttribute("aria-label")).toContain(
      "Its 4 building records and 2 points of interest will be deleted too.",
    );
    expect(ask.getAttribute("aria-label")).toContain(
      "1 Notable Location on it will be taken off and kept",
    );
    expect(api.deleteMap).not.toHaveBeenCalled();
    await user.click(within(ask).getByRole("button", { name: "Delete map" }));
    expect(api.deleteMap).toHaveBeenCalledWith(SETTLEMENT, "map-district-2");
    expect(await screen.findByText("District 2 has no map yet")).toBeTruthy();
    expect(tab(/District 2/).textContent).toContain("No map");
    expect(screen.getByRole("button", { name: "Generate District 2 map" })).toBeTruthy();
  });
});

describe("points of interest", () => {
  it("are placed by clicking the map and named, then shown on the map, legend and directory", async () => {
    await open(capital());
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Add point" }));
    expect(screen.getByText(/Click the map where the point of interest goes/)).toBeTruthy();
    clickAt(onScreen([3, 4]));
    const form = within(inspector()).getByRole("form", { name: "New point of interest" });
    await user.type(within(form).getByLabelText("Point name"), "Old well");
    await user.click(within(form).getByRole("button", { name: "Add point" }));

    const [, mapId, position, label] = api.addPoi.mock.calls[0];
    expect([mapId, label]).toEqual(["map-district-1", "Old well"]);
    expect(position[0]).toBeCloseTo(3, 0);
    expect(position[1]).toBeCloseTo(4, 0);
    expect(await within(inspector()).findByRole("heading", { name: "Old well" })).toBeTruthy();
    const pin = document.querySelector(".map-marker-poi")!;
    expect(pin.classList.contains("is-selected")).toBe(true);
    const legend = screen.getByRole("region", { name: "Legend" });
    expect(within(legend).getByRole("button", { name: "1: Old well, District 1" })).toBeTruthy();
    const directory = screen.getByRole("region", { name: "Directory" });
    expect(within(directory).getByRole("button", { name: /Old well/ })).toBeTruthy();
  });

  it("are moved, renamed and deleted (after confirmation)", async () => {
    const v = capital();
    fake = new FakeDirectory(v);
    fake.addPoi("map-district-1", [0, 0], "Old well", "");
    render(<MapExplorer settlementId={SETTLEMENT} settlementName="Capital" />);
    await screen.findByRole("application", { name: "Map of District 1" });
    const user = userEvent.setup();
    // A pin selects its point.
    const pin = document.querySelector("[data-marker-poi='poi-1']")!;
    fireEvent.mouseDown(pin, { button: 0, clientX: 1, clientY: 1 });
    fireEvent.mouseUp(window, { clientX: 1, clientY: 1 });
    expect(within(inspector()).getByRole("heading", { name: "Old well" })).toBeTruthy();

    await user.click(within(inspector()).getByRole("button", { name: "Move…" }));
    clickAt(onScreen([12, -12]));
    await waitFor(() => expect(api.movePoi).toHaveBeenCalled());
    expect(api.movePoi.mock.calls[0][1]).toBe("poi-1");

    await user.click(within(inspector()).getByRole("button", { name: "Edit point" }));
    const name = within(inspector()).getByLabelText("Point name");
    await user.clear(name);
    await user.type(name, "Dry well");
    await user.click(within(inspector()).getByRole("button", { name: "Save point" }));
    expect(api.updatePoi).toHaveBeenCalledWith(SETTLEMENT, "poi-1", "Dry well", "");
    expect(await within(inspector()).findByRole("heading", { name: "Dry well" })).toBeTruthy();

    await user.click(within(inspector()).getByRole("button", { name: "Delete…" }));
    expect(api.deletePoi).not.toHaveBeenCalled();
    await user.click(within(inspector()).getByRole("button", { name: "Delete point" }));
    expect(api.deletePoi).toHaveBeenCalledWith(SETTLEMENT, "poi-1");
    await waitFor(() => expect(document.querySelector(".map-marker-poi")).toBeNull());
  });
});

describe("the settlement-wide directory", () => {
  it("lists every district's locations and points, and goes to the right district", async () => {
    const v = capital();
    fake = new FakeDirectory(v);
    fake.assign(
      "notable-location-02",
      { map_id: "map-district-2", building_id: "bld-district-2-r1-0002" },
      { move_location: false, replace_occupant: false },
    );
    fake.addPoi("map-district-3", [5, 5], "Ambush site", "");
    render(<MapExplorer settlementId={SETTLEMENT} settlementName="Capital" />);
    await screen.findByRole("application", { name: "Map of District 1" });
    const user = userEvent.setup();
    const directory = () => screen.getByRole("region", { name: "Directory" });

    // Narrowed to the district on screen, then back to the whole settlement.
    await user.click(within(directory()).getByRole("radio", { name: "District 1" }));
    expect(within(directory()).queryByText("Ambush site")).toBeNull();
    expect(within(directory()).getByText("Old Hall")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Settlement directory" }));
    expect(
      within(directory())
        .getByRole("radio", { name: "Whole settlement" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(directory()).getByRole("searchbox", { name: "Search the directory" }),
      ),
    );
    expect(within(directory()).getByRole("region", { name: "District 3" })).toBeTruthy();

    // A point in District 3: switched there, centred and selected.
    await user.click(within(directory()).getByRole("button", { name: /Ambush site/ }));
    expect(await screen.findByRole("application", { name: "Map of District 3" })).toBeTruthy();
    expect(tab("District 3").getAttribute("aria-selected")).toBe("true");
    expect(within(inspector()).getByRole("heading", { name: "Ambush site" })).toBeTruthy();
    const p = onScreen([5, 5]);
    expect(p.clientX).toBeCloseTo(400, 0);
    expect(p.clientY).toBeCloseTo(260, 0);

    // A location in District 2: switched there and its building selected.
    await user.click(within(directory()).getByRole("button", { name: /Dockside Inn/ }));
    expect(await screen.findByRole("application", { name: "Map of District 2" })).toBeTruthy();
    expect(screen.getByTestId("map-selection")).toBeTruthy();
    expect(api.building).toHaveBeenCalledTimes(1);
    expect(api.building).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-district-2",
      "bld-district-2-r1-0002",
    );
  });
});

describe("saved views", () => {
  it("each district opens where it was left, and moving it is remembered once still", async () => {
    const v = capital();
    v.views = [{ map_id: "map-district-2", centre: [6, 6], zoom: 2 }];
    await open(v);
    const user = userEvent.setup();
    const home = viewportOf();
    await user.click(tab("District 2"));
    await screen.findByRole("application", { name: "Map of District 2" });
    const restored = viewportOf();
    expect(restored.k / home.k).toBeCloseTo(2, 1);
    const centre = onScreen([6, 6]);
    expect(centre.clientX).toBeCloseTo(400, 0);
    expect(centre.clientY).toBeCloseTo(260, 0);
    expect(screen.getByLabelText("Zoom 200%")).toBeTruthy();

    // Many wheel steps make one save, after the view settles.
    for (let i = 0; i < 5; i++) fireEvent.wheel(canvas(), { deltaY: -100 });
    expect(api.saveView).not.toHaveBeenCalled();
    await waitFor(() => expect(api.saveView).toHaveBeenCalledTimes(1), {
      timeout: VIEW_SAVE_DELAY * 4,
    });
    const [, saved] = api.saveView.mock.calls[0];
    expect(saved.map_id).toBe("map-district-2");
    expect(saved.zoom).toBeGreaterThan(2);
  });
});

describe("exporting", () => {
  it("exports one district or every drawn district, never a joined city", async () => {
    await open(capital());
    const user = userEvent.setup();
    const exportMock = vi.fn(async () => ({
      dir: "C:/Exports",
      files: ["C:/Exports/Capital - District 1.png"],
    }));
    (api as unknown as { export: typeof exportMock }).export = exportMock;
    await user.click(screen.getByRole("button", { name: "Export…" }));
    const dialog = screen.getByRole("dialog", { name: "Export District 1" });
    await user.click(within(dialog).getByRole("checkbox", { name: /SVG drawing/ }));
    await user.click(within(dialog).getByRole("checkbox", { name: "Legend" }));
    await user.click(within(dialog).getByRole("button", { name: "Export" }));
    expect(exportMock).toHaveBeenCalledWith(SETTLEMENT, {
      map: "map-district-1",
      formats: ["png", "svg"],
      options: { markers: true, legend: false, width_px: 1600 },
    });
    expect(await within(dialog).findByText(/1 file saved in/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Done" }));

    await user.click(screen.getByRole("button", { name: "Export…" }));
    const again = screen.getByRole("dialog", { name: "Export District 1" });
    await user.click(within(again).getByRole("radio", { name: /Every drawn district \(3\)/ }));
    expect(within(again).getByRole("heading", { name: "Export all district maps" })).toBeTruthy();
    await user.click(within(again).getByRole("button", { name: "Export" }));
    expect(exportMock).toHaveBeenLastCalledWith(SETTLEMENT, {
      map: undefined,
      formats: ["png"],
      options: { markers: true, legend: true, width_px: 1600 },
    });
    // Escape closes the dialog only.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("application", { name: "Map of District 1" })).toBeTruthy();
  });
});
