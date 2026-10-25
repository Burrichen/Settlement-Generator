// The no-code editors for generation settings, tags and modifiers: setting
// kinds, options and dependencies; tag explanations and dependencies; the
// Modifier Builder's plain-English explanations; every setting kind in
// Settings and in flow conditions. The core is mocked at `api`.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  Condition,
  ContentSettingView,
  EffectKindSchema,
  FlowContext,
  SettingDef,
  SettingDetail,
  Settings,
  Tag,
  TagDetail,
} from "./api";
import { AppContext, type AppState } from "./app-state";
import { ConditionBuilder } from "./components/flows/ConditionBuilder";

afterEach(cleanup);

const settings = {
  generation_method: "manual",
  generation_presentation: "incremental",
  theme: "modern_dark",
  tag_view: false,
  god_mode: false,
  bare_minimum_buildings: false,
} as unknown as Settings;

let stored: SettingDef[];
let usages: Record<string, { description: string }[]>;

function detail(setting: SettingDef): SettingDetail {
  return {
    setting,
    origin: setting.id.startsWith("user.") ? "custom" : "bundled",
    built_in: !setting.id.startsWith("user."),
    usages: usages[setting.id] ?? [],
    issues: [],
  };
}

const bmb: SettingDef = {
  id: "setting.bareMinimumBuildings",
  name: "Bare Minimum Buildings",
  description: "Always add a Tavern.",
  kind: "toggle",
  default: false,
};

const schema: EffectKindSchema[] = [
  {
    kind: "plus",
    label: "Plus",
    description: "Adds to the roll.",
    executable: true,
    group: "Change the roll",
    fields: [
      { name: "table", label: "Table", kind: { type: "table" }, required: true, help: "" },
      { name: "amount", label: "Amount", kind: { type: "integer" }, required: true, help: "" },
    ],
  },
  {
    kind: "run_flow",
    label: "Run subflow",
    description: "Runs a flow.",
    executable: true,
    group: "Trigger more generation",
    fields: [{ name: "flow", label: "Flow", kind: { type: "flow" }, required: true, help: "" }],
  },
];

const lucky: Tag = {
  id: "tag.lucky",
  name: "Lucky",
  effects: [{ kind: "plus", table: "table.wealth", amount: 2 }],
};

function contentSettings(): ContentSettingView[] {
  return [
    { setting: bmb, value: true },
    {
      setting: { id: "setting.size", name: "Size", kind: "number", default: 3, min: 1 },
      value: 4,
    },
    {
      setting: {
        id: "setting.biomes",
        name: "Biomes",
        kind: "multi_select",
        options: [
          { id: "coast", name: "Coast" },
          { id: "forest", name: "Forest" },
        ],
      },
      value: ["coast"],
    },
    { setting: { id: "setting.motto", name: "Motto", kind: "text" }, value: "Onward" },
    {
      setting: { id: "setting.later", name: "Later", kind: "toggle", status: "planned" },
      value: false,
    },
  ];
}

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    framework: {
      ...actual.framework,
      items: vi.fn(async () =>
        stored.map((s) => ({ kind: "setting", id: s.id, name: s.name, deleted: false })),
      ),
      settingDetail: vi.fn(async (id: string) => detail(stored.find((s) => s.id === id)!)),
      newSetting: vi.fn(async (kind: string) => ({
        id: "user.setting.1",
        name: "New setting",
        kind,
        ...(kind === "multi_select"
          ? {
              options: [
                { id: "option-1", name: "Option 1" },
                { id: "option-2", name: "Option 2" },
              ],
              default: [],
            }
          : { default: false }),
      })),
      checkSetting: vi.fn(async () => ({ coverage: [], issues: [], error: null })),
      saveSetting: vi.fn(async (s: SettingDef) => {
        stored = [...stored.filter((x) => x.id !== s.id), s];
        return detail(s);
      }),
      deleteSetting: vi.fn(async (id: string) => {
        if (usages[id]?.length) throw new Error("This setting is still used: X.");
        stored = stored.filter((s) => s.id !== id);
        return [];
      }),
      resetItem: vi.fn(async () => []),
      settings: vi.fn(async () => contentSettings()),
      setSetting: vi.fn(async () => contentSettings()),
    },
    content: {
      ...actual.content,
      overview: vi.fn(async () => ({
        tables: [],
        tags: [
          {
            id: lucky.id,
            name: lucky.name,
            origin: "bundled",
            built_in: true,
            deleted: false,
            enabled: true,
          },
        ],
        issues: [],
      })),
      tag: vi.fn(async (): Promise<TagDetail> => ({
        tag: lucky,
        origin: "bundled",
        built_in: true,
        usages: [
          {
            kind: "emitted_by",
            location: {},
            description: "Given by Omens › Clover",
          },
          {
            kind: "checked_by",
            location: {},
            description: "Checked by Pipeline stage “Luck” › case 1",
          },
        ],
        issues: [],
      })),
      checkTag: vi.fn(async () => ({ coverage: [], issues: [], error: null })),
      effectSchema: vi.fn(async () => schema),
      explainTag: vi.fn(async (tag: Tag) => ({
        effects: [],
        combined: [],
        informational: (tag.effects ?? []).length === 0,
      })),
      explainEffect: vi.fn(async () => ({
        title: "Plus 2 on Wealth",
        summary: "While Lucky is active: adds 2 to the roll of Wealth before its result is read.",
        preview: ["Roll 1d12", "add 2", "cap at 12", "resolve result"],
        planned: false,
        affects: [{ kind: "table", id: "table.wealth", name: "Wealth" }],
      })),
    },
  };
});

import { framework } from "./api";
import { SettingDefsScreen } from "./screens/SettingDefsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TagsScreen } from "./screens/TagsScreen";

const api = vi.mocked(framework);

function withApp(children: ReactNode) {
  const state: AppState = {
    info: { name: "Settlement Generator", version: "0.1.0" },
    settings,
    updateSettings: async () => null,
    navigate: () => {},
    back: () => {},
    home: () => {},
    menuFocus: { current: 0 },
  };
  return <AppContext.Provider value={state}>{children}</AppContext.Provider>;
}

beforeEach(() => {
  vi.clearAllMocks();
  stored = [bmb];
  usages = {
    [bmb.id]: [{ description: "Used by Pipeline stage “Notable Locations”" }],
  };
});

describe("generation settings editor", () => {
  it("shows what depends on a setting and refuses to delete it while used", async () => {
    const user = userEvent.setup();
    render(withApp(<SettingDefsScreen />));
    expect(await screen.findByText("Used by Pipeline stage “Notable Locations”")).toBeTruthy();
    // Deleting shows where it is used, with links, and deletes nothing.
    await user.click(screen.getByRole("button", { name: "Delete" }));
    const blocked = await screen.findByRole("alert");
    expect(blocked.textContent).toContain("cannot be deleted");
    expect(
      within(blocked).getByRole("list", { name: "Uses of this setting" }).textContent,
    ).toContain("Notable Locations");
    expect(api.deleteSetting).not.toHaveBeenCalled();
  });

  it("creates a multi-select setting with options and defaults", async () => {
    const user = userEvent.setup();
    render(withApp(<SettingDefsScreen />));
    await screen.findByText("Used by Pipeline stage “Notable Locations”");
    await user.selectOptions(screen.getByRole("combobox", { name: "New setting" }), "multi_select");
    const name = await screen.findByRole("textbox", { name: /^Name/ });
    await user.clear(name);
    await user.type(name, "Biomes");
    await user.click(screen.getByRole("button", { name: "Add option" }));
    const options = screen.getByRole("table", { name: "Options" });
    expect(within(options).getAllByRole("row")).toHaveLength(4); // header + 3
    await user.clear(screen.getByRole("textbox", { name: "Option 1 name" }));
    await user.type(screen.getByRole("textbox", { name: "Option 1 name" }), "Coast");
    await user.click(screen.getByRole("checkbox", { name: "Coast is a default" }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(api.saveSetting).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Biomes",
          kind: "multi_select",
          default: ["option-1"],
          options: expect.arrayContaining([{ id: "option-1", name: "Coast" }]),
        }),
      ),
    );
  });
});

describe("settings screen", () => {
  it("offers every kind of content setting and hides planned ones", async () => {
    const user = userEvent.setup();
    render(withApp(<SettingsScreen />));
    // Bare Minimum Buildings is an ordinary content setting now.
    expect(await screen.findByRole("switch", { name: /Bare Minimum Buildings/ })).toBeTruthy();
    expect(screen.getByRole("spinbutton", { name: "Size" })).toBeTruthy();
    expect(screen.getByRole("group", { name: "Biomes" })).toBeTruthy();
    expect(screen.queryByText("Later")).toBeNull();
    await user.click(screen.getByRole("checkbox", { name: "Forest" }));
    expect(api.setSetting).toHaveBeenCalledWith("setting.biomes", ["coast", "forest"]);
    const motto = screen.getByRole("textbox", { name: "Motto" });
    await user.clear(motto);
    await user.type(motto, "Ever on");
    await user.tab();
    expect(api.setSetting).toHaveBeenLastCalledWith("setting.motto", "Ever on");
  });
});

describe("tags and the Modifier Builder", () => {
  it("explains each effect in plain words, with a roll preview", async () => {
    render(withApp(<TagsScreen />));
    const note = await screen.findByRole("note", { name: "In plain words" });
    expect(note.textContent).toContain("adds 2 to the roll of Wealth");
    const preview = within(note).getByRole("list", { name: "What happens to the roll" });
    expect(
      within(preview)
        .getAllByRole("listitem")
        .map((l) => l.textContent),
    ).toEqual(["Roll 1d12", "→ add 2", "→ cap at 12", "→ resolve result."]);
  });

  it("groups effect kinds and lists the tag's dependencies by kind", async () => {
    render(withApp(<TagsScreen />));
    const add = await screen.findByRole("combobox", { name: "Add effect" });
    const groups = within(add)
      .getAllByRole("group")
      .map((g) => g.getAttribute("label"));
    expect(groups).toEqual(["Change the roll", "Trigger more generation"]);
    expect(screen.getByRole("list", { name: "Given by" }).textContent).toBe("Omens › Clover");
    expect(screen.getByRole("list", { name: "Checked by" }).textContent).toBe(
      "Pipeline stage “Luck” › case 1",
    );
  });

  it("says when a tag is informational only", async () => {
    const user = userEvent.setup();
    render(withApp(<TagsScreen />));
    await user.click(await screen.findByRole("button", { name: "Remove Effect 1" }));
    expect(await screen.findByText(/informational only/)).toBeTruthy();
  });
});

describe("conditions on every kind of setting", () => {
  const context: FlowContext = {
    subjects: [],
    tables: [],
    tags: [],
    settings: [
      { id: "setting.size", name: "Size", kind: "number", options: [] },
      {
        id: "setting.biomes",
        name: "Biomes",
        kind: "multi_select",
        options: [
          { id: "coast", name: "Coast" },
          { id: "forest", name: "Forest" },
        ],
      },
      { id: "setting.motto", name: "Motto", kind: "text", options: [] },
    ],
    objects: [],
    flows: [],
    kinds: [],
  };

  it("builds number, multi-select and text tests", async () => {
    const user = userEvent.setup();
    let value: Condition = { setting_between: { setting: "setting.size", min: 2 } };
    const view = () => (
      <ConditionBuilder value={value} onChange={(c) => (value = c)} context={context} label="R" />
    );
    const { rerender } = render(view());
    expect(
      (screen.getByRole("combobox", { name: "R test 1 operator" }) as HTMLSelectElement).value,
    ).toBe("at_least");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "R test 1 operator" }),
      "at_most",
    );
    expect(value).toEqual({ setting_between: { setting: "setting.size", max: 2 } });

    await user.selectOptions(
      screen.getByRole("combobox", { name: "R test 1 subject" }),
      "setting:setting.biomes",
    );
    expect(value).toEqual({ setting_has: { setting: "setting.biomes", value: "coast" } });
    rerender(view());
    await user.selectOptions(
      screen.getByRole("combobox", { name: "R test 1 operator" }),
      "excludes",
    );
    expect(value).toEqual({ not: { setting_has: { setting: "setting.biomes", value: "coast" } } });

    rerender(view());
    await user.selectOptions(
      screen.getByRole("combobox", { name: "R test 1 subject" }),
      "setting:setting.motto",
    );
    expect(value).toEqual({ setting_is: { setting: "setting.motto", value: "" } });
    rerender(view());
    await user.type(screen.getByRole("textbox", { name: "R test 1 value" }), "O");
    expect(value).toEqual({ setting_is: { setting: "setting.motto", value: "O" } });
  });
});
