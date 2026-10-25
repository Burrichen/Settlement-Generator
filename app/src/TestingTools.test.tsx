// Testing, tracing and sharing: the Test panel (Run N for tables and
// flows), Roll details, Import / Export, and "used by" links that open the
// dependent item. The core is mocked at `api`.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Flow, FlowSimulation, ImportPlan, Table, TableSimulation } from "./api";
import { AppContext, type AppState, type ItemTarget } from "./app-state";

afterEach(cleanup);

const tableSim: TableSimulation = {
  runs: 100,
  table: { id: "table.crime", name: "Crime" },
  results: [
    { id: "low", name: "Low", count: 52, actual: 52, expected: 50 },
    { id: "high", name: "High", count: 48, actual: 48, expected: 50 },
    { id: "hidden", name: "Hidden", count: 0, actual: 0, expected: 0 },
  ],
  unreachable: [{ id: "hidden", name: "Hidden" }],
  rerolls: 0,
  modifiers: [{ label: "Plus 1 on Crime (Watchful)", count: 100 }],
  errors: [],
  issues: [],
  note: "100 test rolls with software dice.",
};

const flowSim: FlowSimulation = {
  runs: 1000,
  steps: [
    {
      step: "crime",
      label: "Crime",
      reached: 1000,
      outcomes: [{ id: "low", name: "Low", count: 1000, actual: 100 }],
    },
  ],
  branches: [
    {
      step: "pick",
      label: "pick",
      cases: [
        { id: "case-0", name: "Case 1: Crime is Low", count: 1000, actual: 100 },
        { id: "case-1", name: "Case 2: never", count: 0, actual: 0 },
      ],
    },
  ],
  impossible: ["“pick” › Case 2: never: never taken in 1000 runs"],
  modifiers: [],
  errors: [{ message: "Environment cannot be generated yet", count: 3 }],
  issues: [],
  note: "1000 test runs.",
};

let plan: ImportPlan;

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    content: {
      ...actual.content,
      simulateTable: vi.fn(async () => tableSim),
      overview: vi.fn(async () => ({
        tables: [
          {
            id: "table.crime",
            name: "Crime",
            origin: "bundled",
            built_in: true,
            deleted: false,
            active: true,
            incomplete: false,
            errors: 0,
          },
        ],
        tags: [],
        issues: [],
      })),
    },
    framework: { ...actual.framework, items: vi.fn(async () => []) },
    flows: {
      ...actual.flows,
      simulate: vi.fn(async () => flowSim),
    },
    transfer: {
      exportItems: vi.fn(async () => ({ format: "settlement-generator-content", version: 1 })),
      exportEverything: vi.fn(async () => ({ format: "settlement-generator-content", version: 1 })),
      writePack: vi.fn(
        async (_p: unknown, name: string) => `C:/Docs/Settlement Generator/${name}.json`,
      ),
      exportsFolder: vi.fn(async () => "C:/Docs/Settlement Generator"),
      planImport: vi.fn(async () => plan),
      importPack: vi.fn(async () => plan),
    },
  };
});

import { flows, transfer } from "./api";
import { TestPanel } from "./components/editors/TestPanel";
import { RollDetails } from "./components/generation";
import { UsageLinks } from "./components/editors/UsageLinks";
import { CatalogProvider } from "./components/editors/common";
import { TransferScreen } from "./screens/TransferScreen";

const io = vi.mocked(transfer);

function withApp(children: ReactNode, openItem = vi.fn()) {
  const state: AppState = {
    info: { name: "Settlement Generator", version: "0.1.0" },
    settings: null,
    updateSettings: async () => null,
    navigate: () => {},
    back: () => {},
    home: () => {},
    menuFocus: { current: 0 },
    openItem,
    takeTarget: () => null,
  };
  return (
    <AppContext.Provider value={state}>
      <CatalogProvider>{children}</CatalogProvider>
    </AppContext.Provider>
  );
}

const crime = { id: "table.crime", name: "Crime", dice: "1d4", results: [] } as unknown as Table;
const flow = { id: "user.flow.x", name: "X", steps: [] } as Flow;

beforeEach(() => {
  vi.clearAllMocks();
  plan = {
    name: "Crime pack",
    items: [
      {
        kind: "table",
        id: "table.crime",
        name: "Crime",
        action: { replace: { existing: "Crime" } },
      },
      { kind: "tag", id: "user.tag.a", name: "Watchful", action: "add" },
    ],
    issues: [],
    can_import: true,
    summary: "1 to add, 1 to replace, 0 to skip. Every reference checks out.",
  };
});

describe("the Test panel", () => {
  it("runs a table and compares actual with expected chances", async () => {
    const user = userEvent.setup();
    render(withApp(<TestPanel target={{ kind: "table", table: crime }} disabled={false} />));
    await user.click(screen.getByRole("button", { name: "Run 100" }));
    const table = await screen.findByRole("table");
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows[0].textContent).toContain("Low50.0%52.0%52");
    expect(screen.getByText("Never possible now: Hidden.")).toBeTruthy();
    expect(screen.getByRole("list", { name: "Modifiers used" }).textContent).toContain(
      "Plus 1 on Crime (Watchful): 100 times",
    );
  });

  it("tests a table with tags active (an advanced option)", async () => {
    const user = userEvent.setup();
    render(withApp(<TestPanel target={{ kind: "table", table: crime }} disabled={false} />));
    expect(screen.queryByText("Test with these tags active")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Options…" }));
    expect(screen.getByText("Test with these tags active")).toBeTruthy();
  });

  it("runs a flow and reports branches that never ran and errors", async () => {
    const user = userEvent.setup();
    render(withApp(<TestPanel target={{ kind: "flow", flow }} disabled={false} />));
    await user.click(screen.getByRole("button", { name: "Run 1,000" }));
    await screen.findByText("Branches that never ran");
    expect(vi.mocked(flows.simulate)).toHaveBeenCalledWith(flow, 1000);
    expect(screen.getByText(/never taken in 1000 runs/)).toBeTruthy();
    expect(screen.getByText("3 × Environment cannot be generated yet")).toBeTruthy();
  });

  it("cannot test while the draft has problems", () => {
    render(withApp(<TestPanel target={{ kind: "flow", flow }} disabled />));
    expect((screen.getByRole("button", { name: "Run 100" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
});

describe("Roll details", () => {
  it("shows how each value was reached, on request", async () => {
    const user = userEvent.setup();
    render(
      <RollDetails
        traces={[
          {
            title: "Crime",
            lines: [
              "Base roll: 10 (1d12: 10)",
              "Security modifier: -1",
              "Economy modifier: +2",
              "Final roll: 11",
              "Result: High",
            ],
          },
        ]}
      />,
    );
    await user.click(screen.getByText("Roll details"));
    const lines = screen.getByRole("list", { name: "How Crime was reached" });
    expect(
      within(lines)
        .getAllByRole("listitem")
        .map((l) => l.textContent),
    ).toEqual([
      "Base roll: 10 (1d12: 10)",
      "Security modifier: -1",
      "Economy modifier: +2",
      "Final roll: 11",
      "Result: High",
    ]);
  });
});

describe("dependency links", () => {
  it("open the dependent flow, table or tag", async () => {
    const user = userEvent.setup();
    const openItem = vi.fn<(t: ItemTarget) => void>();
    render(
      withApp(
        <UsageLinks
          label="Used by"
          usages={[
            { description: "Run by Flow “Market” › step “x”", flow: "user.flow.market" },
            {
              description: "Checked by Crime (when it can be rolled)",
              location: { table: "table.crime" },
            },
            { description: "Something without a target" },
          ]}
        />,
        openItem,
      ),
    );
    await user.click(screen.getByRole("button", { name: /Run by Flow/ }));
    expect(openItem).toHaveBeenLastCalledWith({ kind: "flow", id: "user.flow.market" });
    await user.click(screen.getByRole("button", { name: /Checked by Crime/ }));
    expect(openItem).toHaveBeenLastCalledWith({ kind: "table", id: "table.crime" });
    expect(screen.queryByRole("button", { name: /without a target/ })).toBeNull();
  });
});

describe("Import / Export", () => {
  it("exports what the user made and says where the file went", async () => {
    const user = userEvent.setup();
    render(withApp(<TransferScreen />));
    await user.click(screen.getByRole("button", { name: "Export everything I made or changed" }));
    expect(io.exportEverything).toHaveBeenCalledWith("customised");
    expect(
      await screen.findByText("Saved to C:/Docs/Settlement Generator/My content.json"),
    ).toBeTruthy();
  });

  it("exports chosen items with what they need", async () => {
    const user = userEvent.setup();
    render(withApp(<TransferScreen />));
    await user.click(screen.getByRole("button", { name: "Choose what to export…" }));
    await user.click(await screen.findByRole("checkbox", { name: /Crime/ }));
    await user.click(screen.getByRole("button", { name: "Export 1 item" }));
    await waitFor(() =>
      expect(io.exportItems).toHaveBeenCalledWith(
        [{ kind: "table", id: "table.crime" }],
        true,
        "My export",
      ),
    );
  });

  it("checks a file before importing and asks what to do with taken IDs", async () => {
    const user = userEvent.setup();
    render(withApp(<TransferScreen />));
    const file = new File(['{"format":"settlement-generator-content","version":1}'], "crime.json", {
      type: "application/json",
    });
    await user.upload(screen.getByLabelText("Content file to import"), file);
    const items = await screen.findByRole("table", { name: "What will be imported" });
    expect(within(items).getByText("Replace “Crime”")).toBeTruthy();
    expect(io.planImport).toHaveBeenLastCalledWith(expect.any(String), "rename");
    await user.click(screen.getByRole("radio", { name: /Keep mine/ }));
    await waitFor(() => expect(io.planImport).toHaveBeenLastCalledWith(expect.any(String), "skip"));
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(io.importPack).toHaveBeenCalledWith(expect.any(String), "skip");
    expect(await screen.findByText(/^Imported:/)).toBeTruthy();
  });

  it("refuses a file that would break references", async () => {
    plan = {
      ...plan,
      can_import: false,
      issues: [
        {
          severity: "error",
          location: {},
          code: "missing_table",
          message: "flow `x` › step `roll`: references missing table `table.missing`",
        },
      ],
      summary: "It cannot be imported as it is: see the problems below.",
    };
    const user = userEvent.setup();
    render(withApp(<TransferScreen />));
    await user.upload(
      screen.getByLabelText("Content file to import"),
      new File(["{}"], "broken.json", { type: "application/json" }),
    );
    expect(await screen.findByText(/references missing table/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Import" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
});
