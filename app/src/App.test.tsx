// Shell, menu and settings behaviour, driven by the keyboard. The core is
// mocked at the `api` boundary; its own behaviour is tested in Rust.
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "./api";

// jsdom lacks CSS.escape, which user-event uses to walk radio groups.
if (!globalThis.CSS?.escape) {
  Object.assign(globalThis, {
    CSS: { escape: (value: string) => value.replace(/[^\w-]/g, (c) => `\\${c}`) },
  });
}

const stored: { settings: Settings } = {
  settings: {
    generation_method: "automatic",
    generation_presentation: "one_table",
    god_mode: false,
    tag_view: false,
    theme: "modern_dark",
  },
};

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    // An unusual version proves the UI shows real metadata, not a constant.
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "9.8.7" })),
    getRoadmap: vi.fn(async () => [
      { horizon: "next_minor", label: "Next minor update", summary: "Improved name generation" },
      { horizon: "next_major", label: "Next major update", summary: "New tables" },
    ]),
    getSettings: vi.fn(async () => stored.settings),
    saveSettings: vi.fn(async (_previous: Settings, s: Settings) => {
      stored.settings = s;
      return s;
    }),
    exitApp: vi.fn(async () => {}),
    checkForUpdate: vi.fn(async () => null),
  };
});

import { App } from "./App";
import * as api from "./api";

async function renderApp() {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("v9.8.7");
  return user;
}

beforeEach(() => {
  stored.settings = {
    generation_method: "automatic",
    generation_presentation: "one_table",
    god_mode: false,
    tag_view: false,
    theme: "modern_dark",
  };
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("main menu", () => {
  it("lists the destinations with DLC Content disabled and unfocusable", async () => {
    await renderApp();
    const nav = screen.getByRole("navigation", { name: "Main menu" });
    const buttons = within(nav)
      .getAllByRole("button")
      .map((b) => b.textContent?.replace("›", "").trim());
    expect(buttons).toEqual(["Start", "Settings", "View", "Roadmap", "Exit"]);

    const dlc = within(nav).getByText("DLC Content");
    expect(dlc.tagName).not.toBe("BUTTON");
    expect(dlc).toHaveProperty("tabIndex", -1);
    expect(dlc.getAttribute("aria-disabled")).toBe("true");
  });

  it("is driven by arrow keys, skipping DLC Content", async () => {
    const user = await renderApp();
    expect(document.activeElement?.textContent).toContain("Start");
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    expect(document.activeElement?.textContent).toContain("Roadmap");
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toContain("Exit");
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toContain("Start");
    await user.keyboard("{End}");
    expect(document.activeElement?.textContent).toContain("Exit");
  });

  it("opens screens with Enter and returns with Escape to the same item", async () => {
    const user = await renderApp();
    await user.keyboard("{ArrowDown}{Enter}");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeTruthy();
    expect(document.activeElement?.textContent).toContain("Settings");
  });

  it("exits through the core", async () => {
    const user = await renderApp();
    await user.keyboard("{End}{Enter}");
    expect(api.exitApp).toHaveBeenCalled();
  });
});

describe("settings", () => {
  async function openSettings() {
    const user = await renderApp();
    await user.keyboard("{ArrowDown}{Enter}");
    await screen.findByRole("heading", { name: "Settings" });
    return user;
  }

  it("shows the real version", async () => {
    await openSettings();
    expect(screen.getAllByText("v9.8.7").length).toBeGreaterThan(0);
  });

  it("disables Generation Presentation completely while Automatic", async () => {
    const user = await openSettings();
    const presentation = screen.getByRole("radiogroup", { name: "Generation Presentation" });
    const radios = within(presentation).getAllByRole("radio");
    expect(radios.every((r) => (r as HTMLInputElement).disabled)).toBe(true);

    // Keyboard: focus starts on the heading; Tab reaches the method group
    // (Shift+Tab reaches Back). Arrow keys pick Manual.
    await user.tab();
    expect((document.activeElement as HTMLInputElement).value).toBe("automatic");
    await user.keyboard("{ArrowRight}");
    expect(api.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ generation_method: "automatic" }),
      expect.objectContaining({ generation_method: "manual" }),
    );
    expect(radios.every((r) => !(r as HTMLInputElement).disabled)).toBe(true);
    // The kept presentation is restored.
    expect((within(presentation).getByLabelText("One-table") as HTMLInputElement).checked).toBe(
      true,
    );
  });

  it("toggles God Mode and Tag View with the keyboard and saves immediately", async () => {
    const user = await openSettings();
    const god = screen.getByRole("switch", { name: "God Mode" });
    god.focus();
    await user.keyboard(" ");
    expect(api.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ god_mode: false }),
      expect.objectContaining({ god_mode: true }),
    );
    const tags = screen.getByRole("switch", { name: "Tag View" });
    tags.focus();
    await user.keyboard(" ");
    expect(stored.settings.tag_view).toBe(true);
    expect(await screen.findByText("Saved")).toBeTruthy();
  });

  it("switches theme and applies it to the document", async () => {
    const user = await openSettings();
    expect(document.documentElement.dataset.theme).toBe("modern-dark");
    await user.click(screen.getByLabelText("Old-Timey Dark"));
    expect(stored.settings.theme).toBe("old_timey_dark");
    expect(document.documentElement.dataset.theme).toBe("old-timey-dark");
  });

  it("navigates to tags, tables and credits", async () => {
    const user = await openSettings();
    for (const [button, heading] of [
      ["View Tags", "Tags"],
      ["View Tables", "Tables"],
    ]) {
      await user.click(screen.getByRole("button", { name: `Open ${button}` }));
      expect(await screen.findByRole("heading", { name: heading })).toBeTruthy();
      await user.keyboard("{Escape}");
    }
    await user.click(screen.getByRole("button", { name: "Open Credits" }));
    expect(screen.getByText("designed and made by Burrichen")).toBeTruthy();
  });

  it("reports a refused change and keeps the stored value", async () => {
    vi.mocked(api.saveSettings).mockRejectedValueOnce("disabled");
    const user = await openSettings();
    await user.click(screen.getByRole("switch", { name: "God Mode" }));
    expect(await screen.findByText("disabled")).toBeTruthy();
    expect((screen.getByRole("switch", { name: "God Mode" }) as HTMLInputElement).checked).toBe(
      false,
    );
  });
});

describe("roadmap and credits", () => {
  it("shows the roadmap from the core without dates", async () => {
    const user = await renderApp();
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{Enter}");
    expect(await screen.findByText("Improved name generation")).toBeTruthy();
    expect(screen.getByText("New tables")).toBeTruthy();
    expect(screen.getByText("Next minor update")).toBeTruthy();
    expect(screen.getByText("Next major update")).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/\b20\d\d\b|Q[1-4]/);
  });

  it("credits contain exactly the credit line", async () => {
    const user = await renderApp();
    await act(async () => {
      await user.keyboard("{ArrowDown}{Enter}");
    });
    await user.click(await screen.findByRole("button", { name: "Open Credits" }));
    const main = screen.getByRole("main");
    expect(main.textContent?.trim()).toBe("designed and made by Burrichen");
    await user.keyboard("{Escape}");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeTruthy();
  });
});
