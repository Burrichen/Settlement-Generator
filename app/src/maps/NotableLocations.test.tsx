// Notable Locations on the map: the Make Notable Location dialog, moving and
// replacing only on explicit choice, unassigning, reassigning, renaming,
// numbered markers that match the legend, and navigation from the legend
// and the settlement-wide directory (switching district). The core is
// mocked at the `api` boundary by a fake directory with the core's rules.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssignOptions, LocationEdit, PlaceOn, SettlementMapsView } from "./types";
import {
  FakeDirectory,
  SETTLEMENT,
  district,
  districtMap,
  fakeDetails,
  grid,
  location,
  makeMap,
  plannedDistricts,
  plannedSettlement,
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
      editBuilding: vi.fn(),
      regenerateBuilding: vi.fn(),
      resetBuilding: vi.fn(),
      assignLocation: vi.fn(async (_s: string, id: string, t: PlaceOn, o: AssignOptions) =>
        fake.assign(id, t, o),
      ),
      unassignLocation: vi.fn(async (_s: string, id: string) => fake.unassign(id)),
      updateLocation: vi.fn(async (_s: string, id: string, v: number, e: LocationEdit) =>
        fake.update(id, v, e),
      ),
      createLocation: vi.fn(
        async (_s: string, e: LocationEdit, p: PlaceOn | null, o: AssignOptions) =>
          fake.create(e, p, o),
      ),
      deleteLocation: vi.fn(),
      saveView: vi.fn(async () => undefined),
    },
  };
});

import { maps } from "../api";
import { MapExplorer } from "./MapExplorer";

const api = vi.mocked(maps);
const PLAIN = { move_location: false, replace_occupant: false };

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

function town(): SettlementMapsView {
  const map = makeMap("map-settlement", "Testford", grid("map-settlement", 9));
  return withLocations(view([map], plannedSettlement(true)), [
    location(1, { name: "The Golden Goose" }),
    location(2, { location_type: "Trade / Shop" }),
  ]);
}

async function open(v: SettlementMapsView) {
  fake = new FakeDirectory(v);
  render(<MapExplorer settlementId={SETTLEMENT} settlementName="Testford" />);
  await screen.findByRole("application", { name: /Map of/ });
}

function canvas() {
  return screen.getByRole("application", { name: /Map of/ });
}

function onScreen([x, y]: [number, number]) {
  const svg = canvas();
  const k = Number(svg.getAttribute("data-zoom"));
  return {
    clientX: x * k + Number(svg.getAttribute("data-tx")),
    clientY: y * k + Number(svg.getAttribute("data-ty")),
  };
}

/** Selects building `n` (0-based) of the active map and waits for its details. */
async function selectBuilding(n: number) {
  const map = fake.view.maps!.maps.find(
    (m) => canvas().getAttribute("aria-label") === `Map of ${m.label}`,
  )!;
  const point = onScreen(map.geometry.buildings[n].centroid);
  fireEvent.mouseDown(canvas(), { button: 0, ...point });
  fireEvent.mouseUp(window, point);
  await within(inspector()).findByText("Generated", { selector: ".map-state" });
}

function inspector() {
  return screen.getByRole("region", { name: "Location Inspector" });
}

function legendNumbers() {
  const legend = screen.getByRole("region", { name: "Legend" });
  return within(legend)
    .queryAllByRole("button", { name: /^\d+:/ })
    .map((b) => b.getAttribute("aria-label"));
}

function markerNumbers() {
  return [...document.querySelectorAll(".map-marker")].map((m) => m.getAttribute("data-marker"));
}

async function makeNotable(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(inspector()).getByRole("button", { name: "Make Notable Location" }));
  return screen.getByRole("dialog", { name: /a Notable Location/ });
}

describe("making a building a Notable Location", () => {
  it("lists the settlement's locations, searches them, and places the chosen one", async () => {
    await open(town());
    const user = userEvent.setup();
    await selectBuilding(4);
    const dialog = await makeNotable(user);
    expect(within(dialog).getByRole("heading", { name: "Make Building 5 a Notable Location" }));
    const options = () => within(dialog).getAllByRole("option");
    expect(options().map((o) => o.textContent)).toEqual([
      expect.stringContaining("The Golden Goose"),
      expect.stringContaining("Location 2"),
      expect.stringContaining("Custom entry"),
    ]);
    await user.type(within(dialog).getByRole("searchbox"), "shop");
    expect(options()).toHaveLength(2); // Location 2 (Trade / Shop) and Custom entry

    await user.clear(within(dialog).getByRole("searchbox"));
    await user.click(within(dialog).getByRole("option", { name: /The Golden Goose/ }));
    expect(api.assignLocation).not.toHaveBeenCalled(); // nothing until confirmed
    await user.click(within(dialog).getByRole("button", { name: "Make Notable Location" }));

    expect(api.assignLocation).toHaveBeenCalledWith(
      SETTLEMENT,
      "notable-location-01",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0005" },
      PLAIN,
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // A numbered marker, the same number in the legend, and the inspector link.
    expect(markerNumbers()).toEqual(["1"]);
    expect(legendNumbers()).toEqual(["1: The Golden Goose, Hospitality"]);
    expect(within(inspector()).getByText("Notable location · marker 1")).toBeTruthy();
    // The building's own details were not touched.
    expect(api.editBuilding).not.toHaveBeenCalled();
  });

  it("adds a custom location placed on the building", async () => {
    await open(town());
    const user = userEvent.setup();
    await selectBuilding(0);
    const dialog = await makeNotable(user);
    await user.click(within(dialog).getByRole("option", { name: /Custom entry/ }));
    await user.type(within(dialog).getByLabelText("Name"), "Blackrock Keep");
    await user.type(within(dialog).getByLabelText("Type"), "Castle");
    await user.click(within(dialog).getByRole("button", { name: "Make Notable Location" }));
    expect(api.createLocation).toHaveBeenCalledWith(
      SETTLEMENT,
      { name: "Blackrock Keep", location_type: "Castle", notes: "" },
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0001" },
      PLAIN,
    );
    await waitFor(() => expect(legendNumbers()).toEqual(["1: Blackrock Keep, Castle"]));
  });

  it("moves a placed location or replaces a building's location only when that is chosen", async () => {
    await open(town());
    const user = userEvent.setup();
    await selectBuilding(0);
    let dialog = await makeNotable(user);
    await user.click(within(dialog).getByRole("option", { name: /The Golden Goose/ }));
    await user.click(within(dialog).getByRole("button", { name: "Make Notable Location" }));
    await waitFor(() => expect(markerNumbers()).toEqual(["1"]));

    // On another building, the same location is offered as a move.
    await selectBuilding(1);
    dialog = await makeNotable(user);
    expect(within(dialog).getByRole("option", { name: /The Golden Goose/ }).textContent).toContain(
      "On Building 1, Testford",
    );
    await user.click(within(dialog).getByRole("option", { name: /The Golden Goose/ }));
    expect(within(dialog).getByText(/will move from Building 1, Testford/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Move here" }));
    expect(api.assignLocation).toHaveBeenLastCalledWith(
      SETTLEMENT,
      "notable-location-01",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0002" },
      { move_location: true, replace_occupant: false },
    );
    await waitFor(() => expect(markerNumbers()).toEqual(["1"])); // moved, not doubled

    // Another location onto that building replaces the first, which is kept.
    dialog = await makeNotable(user);
    await user.click(within(dialog).getByRole("option", { name: /Location 2/ }));
    expect(within(dialog).getByText(/The Golden Goose will be taken off this building/));
    await user.click(within(dialog).getByRole("button", { name: "Replace" }));
    expect(api.assignLocation).toHaveBeenLastCalledWith(
      SETTLEMENT,
      "notable-location-02",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0002" },
      { move_location: false, replace_occupant: true },
    );
    await waitFor(() => expect(legendNumbers()).toEqual(["2: Location 2, Trade / Shop"]));
    const directory = screen.getByRole("region", { name: "Directory" });
    expect(within(directory).getByText(/1 of 2 Notable Locations placed/)).toBeTruthy();
  });

  it("refuses a location from another district", async () => {
    const maps2 = [districtMap(1), districtMap(2)];
    await open(
      withLocations(view(maps2, plannedDistricts([true, true])), [
        location(1, { district: district(2), name: "Dockside Inn" }),
      ]),
    );
    const user = userEvent.setup();
    await selectBuilding(0);
    const dialog = await makeNotable(user);
    const option = within(dialog).getByRole("option", { name: /Dockside Inn/ });
    expect(option.getAttribute("aria-disabled")).toBe("true");
    expect(option.textContent).toContain("Belongs to District 2");
    await user.click(option);
    expect(
      (within(dialog).getByRole("button", { name: "Make Notable Location" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("map-selection")).toBeTruthy(); // Esc closed only the dialog
  });
});

describe("a placed location", () => {
  async function placed() {
    fake = new FakeDirectory(town());
    fake.assign(
      "notable-location-01",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0001" },
      PLAIN,
    );
    fake.assign(
      "notable-location-02",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0003" },
      PLAIN,
    );
    render(<MapExplorer settlementId={SETTLEMENT} settlementName="Testford" />);
    await screen.findByRole("application", { name: /Map of/ });
  }

  it("draws numbered pins that match the legend, and a pin selects its building", async () => {
    await placed();
    expect(markerNumbers()).toEqual(["1", "2"]);
    expect(legendNumbers()).toEqual([
      "1: The Golden Goose, Hospitality",
      "2: Location 2, Trade / Shop",
    ]);
    const pin = document.querySelector('[data-marker="2"]')!;
    expect(pin.getAttribute("data-marker-building")).toBe("bld-settlement-r1-0003");
    fireEvent.mouseDown(pin, { button: 0, clientX: 5, clientY: 5 });
    fireEvent.mouseUp(window, { clientX: 5, clientY: 5 });
    expect(await within(inspector()).findByText("Notable location · marker 2")).toBeTruthy();
  });

  it("unassigns after confirmation, keeping the location and its number", async () => {
    await placed();
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Unassign…" }));
    const ask = within(inspector()).getByRole("alertdialog", { name: "Unassign location" });
    expect(within(ask).getByText(/keeps its name, notes and marker number 1/)).toBeTruthy();
    await user.click(within(ask).getByRole("button", { name: "Unassign" }));
    expect(api.unassignLocation).toHaveBeenCalledWith(SETTLEMENT, "notable-location-01");
    await waitFor(() => expect(markerNumbers()).toEqual(["2"]));
    expect(legendNumbers()).toEqual(["2: Location 2, Trade / Shop"]); // no renumbering
    expect(within(inspector()).getByText("None")).toBeTruthy();
    const directory = screen.getByRole("region", { name: "Directory" });
    expect(within(directory).getByText("The Golden Goose")).toBeTruthy();
  });

  it("reassigns by choosing another building, and confirms first", async () => {
    await placed();
    const user = userEvent.setup();
    await selectBuilding(0);
    await user.click(within(inspector()).getByRole("button", { name: "Reassign…" }));
    expect(screen.getByText(/Choose the building to move/)).toBeTruthy();
    const target = fake.view.maps!.maps[0].geometry.buildings[8];
    const point = onScreen(target.centroid);
    fireEvent.mouseDown(canvas(), { button: 0, ...point });
    fireEvent.mouseUp(window, point);
    const bar = screen.getByRole("alertdialog", { name: /Move The Golden Goose to Building 9/ });
    expect(api.assignLocation).not.toHaveBeenCalled();
    await user.click(within(bar).getByRole("button", { name: "Move" }));
    expect(api.assignLocation).toHaveBeenCalledWith(
      SETTLEMENT,
      "notable-location-01",
      { map_id: "map-settlement", building_id: "bld-settlement-r1-0009" },
      { move_location: true, replace_occupant: false },
    );
    await waitFor(() =>
      expect(
        document.querySelector('[data-marker="1"]')!.getAttribute("data-marker-building"),
      ).toBe("bld-settlement-r1-0009"),
    );
    expect(screen.queryByText(/Choose the building to move/)).toBeNull();
  });

  it("renames the location through its record", async () => {
    await placed();
    const user = userEvent.setup();
    await selectBuilding(2);
    await user.click(within(inspector()).getByRole("button", { name: "Edit location" }));
    await user.type(within(inspector()).getByLabelText("Location name"), "Pellam's Wares");
    await user.type(within(inspector()).getByLabelText("Location notes"), "Sells maps.");
    await user.click(within(inspector()).getByRole("button", { name: "Save location" }));
    expect(api.updateLocation).toHaveBeenCalledWith(SETTLEMENT, "notable-location-02", 1, {
      name: "Pellam's Wares",
      location_type: null,
      notes: "Sells maps.",
    });
    await waitFor(() =>
      expect(legendNumbers()).toEqual([
        "1: The Golden Goose, Hospitality",
        "2: Pellam's Wares, Trade / Shop",
      ]),
    );
    expect(within(inspector()).getByText("Sells maps.")).toBeTruthy();
  });

  it("a legend entry selects, centres and inspects its building", async () => {
    await placed();
    const user = userEvent.setup();
    const legend = screen.getByRole("region", { name: "Legend" });
    await user.click(within(legend).getByRole("button", { name: /^2:/ }));
    expect(await within(inspector()).findByText("Notable location · marker 2")).toBeTruthy();
    const b = fake.view.maps!.maps[0].geometry.buildings[2];
    const p = onScreen(b.centroid);
    expect(p.clientX).toBeCloseTo(400, 0);
    expect(p.clientY).toBeCloseTo(260, 0);
  });
});

describe("cities", () => {
  it("the settlement-wide directory switches to the right district and shows the building", async () => {
    const maps2 = [districtMap(1), districtMap(2)];
    const v = withLocations(view(maps2, plannedDistricts([true, true])), [
      location(1, { district: district(1), name: "Old Hall" }),
      location(2, { district: district(2), name: "Dockside Inn" }),
    ]);
    fake = new FakeDirectory(v);
    fake.assign(
      "notable-location-02",
      { map_id: "map-district-2", building_id: "bld-district-2-r1-0004" },
      PLAIN,
    );
    render(<MapExplorer settlementId={SETTLEMENT} settlementName="Testford" />);
    await screen.findByRole("application", { name: "Map of District 1" });
    const user = userEvent.setup();

    const directory = screen.getByRole("region", { name: "Directory" });
    // Grouped by district, placed or not.
    expect(within(directory).getByRole("region", { name: "District 1" })).toBeTruthy();
    expect(within(directory).getByText(/Old Hall/)).toBeTruthy();
    await user.click(within(directory).getByRole("button", { name: /Dockside Inn/ }));

    expect(await screen.findByRole("application", { name: "Map of District 2" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "District 2" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    expect(await within(inspector()).findByText("Notable location · marker 1")).toBeTruthy();
    expect(screen.getByTestId("map-selection")).toBeTruthy();
    expect(legendNumbers()).toEqual(["1: Dockside Inn, Hospitality, District 2"]);
    expect(api.building).toHaveBeenCalledWith(
      SETTLEMENT,
      "map-district-2",
      "bld-district-2-r1-0004",
    );
  });
});
