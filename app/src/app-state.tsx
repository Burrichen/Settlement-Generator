import { createContext, useContext, type MutableRefObject } from "react";
import type { AppInfo, Settings } from "./api";

export type Screen =
  "menu" | "start" | "settings" | "view" | "roadmap" | "credits" | "tags" | "tables";

export interface AppState {
  info: AppInfo | null;
  settings: Settings | null;
  /** Saves immediately; resolves to an error message, or null on success. */
  updateSettings: (next: Settings) => Promise<string | null>;
  navigate: (screen: Screen) => void;
  back: () => void;
  /** Returns to the Main Menu, clearing the screen history. */
  home: () => void;
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
