// Naming content in View Tables: approved names edited one variant at a
// time, missing names reported with the table, result and variant, and each
// naming method's join and spelling exceptions. The core is mocked at `api`.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ContentOverview,
  DraftCheck,
  NamingMethodDetail,
  Settings,
  Table,
  TableDetail,
} from "./api";

const settings: Settings = {
  generation_method: "automatic",
  generation_presentation: "incremental",
  god_mode: false,
  tag_view: false,
  theme: "modern_dark",
  bare_minimum_buildings: false,
};

const founders: Table = {
  id: "table.naming.ordon.founder",
  name: "Ordon Manorial: Historical Person or Landholder",
  dice: "1d2",
  results: [
    {
      id: "oswald",
      name: "Oswald",
      range: 1,
      approved_names: { a: "Oswalden", b: "Oswaldwick" },
    },
    {
      id: "godwin",
      name: "Godwin",
      range: 2,
      approved_names: { a: "Godwinsbury", b: "Godwinham" },
    },
  ],
};

const methodTable: Table = {
  id: "table.naming.ordon.method",
  name: "Ordon Naming Method",
  dice: "1d100",
  results: [
    { id: "geographic", name: "Old English & Landscape", range: [1, 42] },
    { id: "founder", name: "Founders, Manors & Estates", range: [43, 100] },
  ],
};

const scheme = { id: "naming.kurovia.ordon", name: "Ordon" };

const geographic: NamingMethodDetail = {
  scheme,
  method: { id: "geographic", name: "Old English & Landscape" },
  join: { style: "compound" },
  exceptions: [{ results: ["ash", "ham"], name: "Asham", note: "Historical-looking contraction" }],
  components: [
    {
      table: { id: "table.naming.ordon.firstElement", name: "First Element" },
      results: [
        { id: "ash", name: "Ash" },
        { id: "oak", name: "Oak" },
      ],
    },
    {
      table: { id: "table.naming.ordon.secondElement", name: "Second Element" },
      results: [
        { id: "ham", name: "ham" },
        { id: "ton", name: "ton" },
      ],
    },
  ],
  modified: false,
  built_in: true,
};

const founderMethod: NamingMethodDetail = {
  scheme,
  method: { id: "founder", name: "Founders, Manors & Estates" },
  join: {
    style: "curated_variant",
    source_field: "approved_names",
    variant_from_result_id: 1,
    on_missing: "error",
  },
  exceptions: [],
  components: [
    { table: { id: founders.id, name: founders.name }, results: [] },
    { table: { id: "table.naming.ordon.founderEnding", name: "Approved form" }, results: [] },
  ],
  modified: false,
  built_in: true,
};

function detail(table: Table, extra: Partial<TableDetail> = {}): TableDetail {
  return {
    table,
    origin: "bundled",
    built_in: true,
    coverage: [],
    issues: [],
    ...extra,
  };
}

const founderDetail = (table: Table = founders) =>
  detail(table, {
    approved_names: [
      {
        scheme,
        method: { id: "founder", name: "Founders, Manors & Estates" },
        variant_table: { id: "table.naming.ordon.founderEnding", name: "Approved form" },
        variants: [
          { id: "a", name: "A" },
          { id: "b", name: "B" },
        ],
      },
    ],
  });

const methodDetail = (methods = [geographic, founderMethod]) =>
  detail(methodTable, { naming_methods: methods });

let overview: ContentOverview;
let nextCheck: DraftCheck;
let nextNamingCheck: DraftCheck;

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
      table: vi.fn(async (id: string) => (id === founders.id ? founderDetail() : methodDetail())),
      tag: vi.fn(),
      checkTable: vi.fn(async () => nextCheck),
      checkTag: vi.fn(),
      saveTable: vi.fn(async (t: Table) => founderDetail(t)),
      saveTag: vi.fn(),
      newTable: vi.fn(),
      newTag: vi.fn(),
      newId: vi.fn(async (kind: string) => `user.${kind}.1`),
      deleteTable: vi.fn(),
      deleteTag: vi.fn(),
      resetTable: vi.fn(async () => overview),
      resetTag: vi.fn(),
      resetAll: vi.fn(async () => overview),
      effectSchema: vi.fn(async () => []),
      checkNamingMethod: vi.fn(async () => nextNamingCheck),
      saveNamingMethod: vi.fn(),
      resetNamingMethod: vi.fn(),
    },
  };
});

import { App } from "./App";
import { content } from "./api";

const api = vi.mocked(content);

const listed = (t: Table) => ({
  id: t.id,
  name: t.name,
  origin: "bundled" as const,
  built_in: true,
  deleted: false,
  active: true,
  incomplete: false,
  errors: 0,
});

async function openTables(first: Table) {
  overview = { tables: [listed(first)], tags: [], issues: [] };
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v0.1.0");
  await user.keyboard("{ArrowDown}{Enter}"); // Settings
  await user.click(await screen.findByRole("button", { name: "Open View Tables" }));
  return user;
}

beforeEach(() => {
  vi.clearAllMocks();
  nextCheck = { coverage: [], issues: [], error: null };
  nextNamingCheck = { coverage: [], issues: [], error: null };
});

afterEach(cleanup);

describe("approved names", () => {
  it("edits each approved variant on its own", async () => {
    const user = await openTables(founders);
    const a = (await screen.findByLabelText("Oswald: approved name A")) as HTMLInputElement;
    const b = screen.getByLabelText("Oswald: approved name B") as HTMLInputElement;
    expect(a.value).toBe("Oswalden");
    expect(b.value).toBe("Oswaldwick");
    await user.clear(b);
    await user.type(b, "Oswaldby");
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    await user.click(save);
    const saved = api.saveTable.mock.calls[0][0];
    expect(saved.results[0].approved_names).toEqual({ a: "Oswalden", b: "Oswaldby" });
    expect(saved.results[1].approved_names).toEqual({ a: "Godwinsbury", b: "Godwinham" });
  });

  it("reports a deleted variant name and refuses to save", async () => {
    const user = await openTables(founders);
    const b = await screen.findByLabelText("Oswald: approved name B");
    nextCheck = {
      coverage: [],
      issues: [
        {
          severity: "error",
          location: { table: founders.id, result: "oswald", naming_scheme: scheme.id },
          code: "approved_name_missing",
          message:
            'naming scheme `naming.kurovia.ordon` › table `table.naming.ordon.founder` › result `oswald`: no approved name for variant "B" (`b` of table `table.naming.ordon.founderEnding`); add one before this table can generate names',
        },
      ],
      error: null,
    };
    await user.clear(b);
    expect(await screen.findByText(/— missing/)).toBeTruthy();
    expect(await screen.findByText(/no approved name for variant "B"/)).toBeTruthy();
    expect(screen.getByText(/result `oswald`/)).toBeTruthy();
    const save = screen.getByRole("button", { name: "Save changes" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    // The emptied name is removed from the draft, not saved as "".
    const checked = api.checkTable.mock.calls.at(-1)![0];
    expect(checked.results[0].approved_names).toEqual({ a: "Oswalden" });
  });
});

describe("naming methods", () => {
  it("lists each method with its join and edits spelling exceptions", async () => {
    const user = await openTables(methodTable);
    const toggle = await screen.findByRole("button", { name: "Old English & Landscape" });
    expect(screen.getByText(/compound · 1 exception$/)).toBeTruthy();
    expect(screen.getByText(/curated_variant · 0 exceptions/)).toBeTruthy();
    await user.click(toggle);
    const name = screen.getByLabelText(
      "Old English & Landscape: exception 1 name",
    ) as HTMLInputElement;
    expect(name.value).toBe("Asham");
    expect(
      (screen.getByLabelText("Old English & Landscape: exception 1 note") as HTMLInputElement)
        .value,
    ).toBe("Historical-looking contraction");

    await user.click(screen.getByRole("button", { name: "Add exception" }));
    await user.selectOptions(
      screen.getByLabelText("Old English & Landscape: exception 2 part 1"),
      "oak",
    );
    await user.selectOptions(
      screen.getByLabelText("Old English & Landscape: exception 2 part 2"),
      "ton",
    );
    await user.type(screen.getByLabelText("Old English & Landscape: exception 2 name"), "Ockton");
    const save = screen.getByRole("button", {
      name: "Save Old English & Landscape",
    }) as HTMLButtonElement;
    await waitFor(() => expect(save.disabled).toBe(false));
    api.saveNamingMethod.mockResolvedValueOnce(
      methodDetail([
        {
          ...geographic,
          modified: true,
          exceptions: [...geographic.exceptions, { results: ["oak", "ton"], name: "Ockton" }],
        },
        founderMethod,
      ]),
    );
    await user.click(save);
    expect(api.saveNamingMethod).toHaveBeenCalledWith(
      scheme.id,
      "geographic",
      { style: "compound" },
      [geographic.exceptions[0], { results: ["oak", "ton"], name: "Ockton" }],
    );
    expect(await screen.findByText("Naming method saved")).toBeTruthy();
    expect(screen.getByText(/· customised/)).toBeTruthy();

    // A customised method can be reset to its default.
    api.resetNamingMethod.mockResolvedValueOnce(methodDetail());
    await user.click(
      screen.getByRole("button", { name: "Reset Old English & Landscape to default" }),
    );
    expect(api.resetNamingMethod).toHaveBeenCalledWith(scheme.id, "geographic");
  });

  it("shows a curated method's parts and refuses an invalid join", async () => {
    const user = await openTables(methodTable);
    await user.click(await screen.findByRole("button", { name: "Founders, Manors & Estates" }));
    const variant = screen.getByLabelText(
      "Founders, Manors & Estates: variant part",
    ) as HTMLSelectElement;
    expect(variant.value).toBe("1");
    expect(
      within(variant)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual([`Part 1: ${founders.name}`, "Part 2: Approved form"]);
    nextNamingCheck = {
      coverage: [],
      issues: [
        {
          severity: "error",
          location: { naming_scheme: scheme.id, result: "founder" },
          code: "curated_variant_components",
          message:
            "curated_variant reads its source from component 0 and its variant from component 0",
        },
      ],
      error: null,
    };
    await user.selectOptions(variant, "0");
    expect(await screen.findByText(/reads its source from component 0/)).toBeTruthy();
    const save = screen.getByRole("button", {
      name: "Save Founders, Manors & Estates",
    }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
  });
});
