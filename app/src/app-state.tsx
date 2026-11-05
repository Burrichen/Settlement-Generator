import { createContext, useContext, type MutableRefObject } from "react";
import type { AccentPalette, AppInfo, Settings } from "./api";

export type Screen =
  | "menu"
  | "start"
  | "settings"
  | "view"
  | "roadmap"
  | "credits"
  | "tags"
  | "tables"
  | "flows"
  | "setting-defs"
  | "transfer";

/** An item to open in its editor, e.g. from a "used by" link. */
export interface ItemTarget {
  kind: "table" | "tag" | "flow" | "setting";
  id: string;
}

export interface AppState {
  info: AppInfo | null;
  settings: Settings | null;
  /** Saves immediately; resolves to an error message, or null on success. */
  updateSettings: (next: Settings) => Promise<string | null>;
  navigate: (screen: Screen) => void;
  back: () => void;
  /** Returns to the Main Menu, clearing the screen history. */
  home: () => void;
  /** Opens an item's editor (its screen, with it selected). */
  openItem?: (target: ItemTarget) => void;
  /** The item a screen was opened for by `openItem`; `null` if none. */
  takeTarget?: (kind: ItemTarget["kind"]) => string | null;
  /** Index of the last focused main-menu item, restored on return. */
  menuFocus: MutableRefObject<number>;
}

export const AppContext = createContext<AppState | null>(null);

export function useApp(): AppState {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppContext");
  return ctx;
}

/** Theme IDs map to `data-theme` values used by tokens.css. */
export function themeAttribute(theme: Settings["theme"]): string {
  return theme.replace(/_/g, "-");
}

/** The accent palettes in the order Settings offers them. */
export const ACCENT_PALETTES: { id: AccentPalette; name: string }[] = [
  { id: "ember", name: "Ember" },
  { id: "brass", name: "Brass" },
  { id: "amethyst", name: "Amethyst" },
  { id: "verdigris", name: "Verdigris" },
  { id: "lapis", name: "Lapis" },
  { id: "moss", name: "Moss" },
  { id: "glacier", name: "Glacier" },
];

/** The settings field that holds a theme's accent choice. */
export function accentKey(
  theme: Settings["theme"],
): "modern_dark_accent" | "old_timey_dark_accent" {
  return theme === "modern_dark" ? "modern_dark_accent" : "old_timey_dark_accent";
}

/** The active theme's accent palette, or that theme's own until one is chosen. */
export function activeAccent(settings: Settings): AccentPalette {
  return (
    settings[accentKey(settings.theme)] ?? (settings.theme === "modern_dark" ? "ember" : "brass")
  );
}
