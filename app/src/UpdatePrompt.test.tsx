// The update prompt: offered on the Main Menu, never forced, and the data
// error view shown when the user database cannot be opened.
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { InstallProgress, Settings, UpdateOffer } from "./api";

const settings: Settings = {
  generation_method: "automatic",
  generation_presentation: "one_table",
  god_mode: false,
  tag_view: false,
  theme: "modern_dark",
};

const offer: UpdateOffer = { current_version: "0.1.0", version: "0.2.0", notes: "Name generation" };

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getAppInfo: vi.fn(async () => ({ name: "Settlement Generator", version: "0.1.0" })),
    getSettings: vi.fn(async () => settings),
    saveSettings: vi.fn(async (_previous: Settings, s: Settings) => s),
    exitApp: vi.fn(async () => {}),
    checkForUpdate: vi.fn(async () => null),
    installUpdate: vi.fn(async () => {}),
  };
});

import { App } from "./App";
import * as api from "./api";

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

async function renderWithOffer() {
  vi.mocked(api.checkForUpdate).mockResolvedValueOnce(offer);
  const user = userEvent.setup();
  render(<App />);
  return { user, dialog: await screen.findByRole("dialog", { name: "Update available" }) };
}

describe("update prompt", () => {
  it("shows the current and available versions with Update and Later", async () => {
    const { dialog } = await renderWithOffer();
    expect(within(dialog).getByText("Current version").nextSibling?.textContent).toBe("v0.1.0");
    expect(within(dialog).getByText("Available version").nextSibling?.textContent).toBe("v0.2.0");
    expect(within(dialog).getByRole("button", { name: "Update" })).toBe(document.activeElement);
    expect(within(dialog).getByRole("button", { name: "Later" })).toBeTruthy();
    expect(within(dialog).getByText("Name generation")).toBeTruthy();
    expect(api.installUpdate).not.toHaveBeenCalled();
  });

  it("does nothing when up to date or when the check fails", async () => {
    vi.mocked(api.checkForUpdate).mockRejectedValueOnce("offline");
    render(<App />);
    await screen.findByText("v0.1.0");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("Later dismisses without installing and returns focus to the menu", async () => {
    const { user, dialog } = await renderWithOffer();
    await user.click(within(dialog).getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.installUpdate).not.toHaveBeenCalled();
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });

  it("Escape means Later and does not reach the screen behind", async () => {
    const { user } = await renderWithOffer();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.installUpdate).not.toHaveBeenCalled();
    expect(screen.getByRole("navigation", { name: "Main menu" })).toBeTruthy();
  });

  it("keeps keyboard focus inside the dialog", async () => {
    const { user, dialog } = await renderWithOffer();
    for (let i = 0; i < 5; i++) {
      await user.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });

  it("installs only when Update is chosen, showing progress", async () => {
    let report: (p: InstallProgress) => void = () => {};
    vi.mocked(api.installUpdate).mockImplementationOnce((onProgress) => {
      report = onProgress;
      return new Promise(() => {}); // the installer closes the app
    });
    const { user, dialog } = await renderWithOffer();
    await user.click(within(dialog).getByRole("button", { name: "Update" }));
    expect(api.installUpdate).toHaveBeenCalledTimes(1);

    act(() => report({ stage: "downloading", downloaded: 25, total: 100 }));
    expect(within(dialog).getByText("Downloading update (25%)")).toBeTruthy();
    expect(within(dialog).getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
    act(() => report({ stage: "backing_up" }));
    expect(within(dialog).getByText("Backing up your data…")).toBeTruthy();
    act(() => report({ stage: "installing" }));
    expect(within(dialog).getByText(/will close and restart/)).toBeTruthy();

    // Escape cannot abandon an install in progress.
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("reports a failed install and can be closed", async () => {
    vi.mocked(api.installUpdate).mockRejectedValueOnce(
      "Your data could not be backed up, so the update was not installed: disk full",
    );
    const { user, dialog } = await renderWithOffer();
    await user.click(within(dialog).getByRole("button", { name: "Update" }));
    expect((await within(dialog).findByRole("alert")).textContent).toContain("disk full");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("waits for the Main Menu instead of interrupting another screen", async () => {
    let resolve: (o: UpdateOffer) => void = () => {};
    vi.mocked(api.checkForUpdate).mockReturnValueOnce(new Promise((r) => (resolve = r)));
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("v0.1.0");
    await user.keyboard("{ArrowDown}{Enter}");
    await screen.findByRole("heading", { name: "Settings" });

    await act(async () => resolve(offer));
    expect(screen.queryByRole("dialog")).toBeNull();

    await user.keyboard("{Escape}");
    expect(await screen.findByRole("dialog", { name: "Update available" })).toBeTruthy();
  });
});

describe("data errors", () => {
  it("explains a failed upgrade, with the backup location, and offers Exit", async () => {
    const message =
      "Your saved data could not be upgraded for this version: … Nothing was changed … backup … C:\Users\me\AppData\Roaming\SettlementGenerator\backups\pre-migration-x.sqlite3";
    vi.mocked(api.getSettings).mockRejectedValueOnce(message);
    const user = userEvent.setup();
    render(<App />);
    expect(
      await screen.findByRole("heading", { name: "Your data could not be opened" }),
    ).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe(message);
    expect(screen.queryByRole("navigation", { name: "Main menu" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Exit" }));
    expect(api.exitApp).toHaveBeenCalled();
  });
});
