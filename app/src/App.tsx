import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  checkForUpdate,
  exitApp,
  getAppInfo,
  getSettings,
  saveSettings,
  type AppInfo,
  type Settings,
  type UpdateOffer,
} from "./api";
import { AppContext, themeAttribute, type AppState, type Screen } from "./app-state";
import { MainMenu } from "./screens/MainMenu";
import { SettingsScreen } from "./screens/SettingsScreen";
import { RoadmapScreen } from "./screens/RoadmapScreen";
import { CreditsScreen } from "./screens/CreditsScreen";
import { StartScreen } from "./screens/StartScreen";
import { ViewScreen } from "./screens/ViewScreen";
import { TagsScreen } from "./screens/TagsScreen";
import { TablesScreen } from "./screens/TablesScreen";
import { UpdatePrompt } from "./components/UpdatePrompt";

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [stack, setStack] = useState<Screen[]>(["menu"]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [dataError, setDataError] = useState<string | null>(null);
  const [update, setUpdate] = useState<UpdateOffer | null>(null);
  const menuFocus = useRef(0);

  useEffect(() => {
    getAppInfo().then(setInfo, (e: unknown) => setLoadError(String(e)));
    // Settings come from the user database: if they cannot load, nothing can.
    getSettings().then(setSettings, (e: unknown) => setDataError(String(e)));
    // Update checks fail quietly (for example when offline).
    checkForUpdate().then(setUpdate, () => {});
  }, []);
  const dismissUpdate = useCallback(() => setUpdate(null), []);

  // Apply the theme from settings (Modern Dark until they load).
  useEffect(() => {
    document.documentElement.dataset.theme = themeAttribute(settings?.theme ?? "modern_dark");
  }, [settings?.theme]);

  const navigate = useCallback((screen: Screen) => setStack((s) => [...s, screen]), []);
  const back = useCallback(() => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s)), []);
  const home = useCallback(() => setStack(["menu"]), []);

  const updateSettings = useCallback(
    async (next: Settings) => {
      const previous = settings;
      setSettings(next);
      try {
        setSettings(await saveSettings(previous ?? next, next));
        return null;
      } catch (e) {
        setSettings(previous);
        return String(e);
      }
    },
    [settings],
  );

  // Esc goes back from any screen.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) back();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [back]);

  const state: AppState = useMemo(
    () => ({ info, settings, updateSettings, navigate, back, home, menuFocus }),
    [info, settings, updateSettings, navigate, back, home],
  );

  const screen = stack[stack.length - 1];
  // The prompt waits for the Main Menu so it never interrupts work.
  const prompting = update !== null && (screen === "menu" || dataError !== null);
  return (
    <AppContext.Provider value={state}>
      <div className="app-root" inert={prompting}>
        {loadError && (
          <div role="alert" className="status-line is-error" style={{ padding: "8px 24px" }}>
            {loadError}
          </div>
        )}
        {dataError ? <DataError message={dataError} /> : <Screens screen={screen} />}
      </div>
      {prompting && <UpdatePrompt offer={update} onLater={dismissUpdate} />}
    </AppContext.Provider>
  );
}

/** Shown when the user database cannot be opened, e.g. after a failed upgrade. */
function DataError({ message }: { message: string }) {
  return (
    <div className="menu-screen">
      <div className="menu-center">
        <section className="data-error" aria-labelledby="data-error-title">
          <h1 id="data-error-title" className="display">
            Your data could not be opened
          </h1>
          <p role="alert" className="data-error-message">
            {message}
          </p>
          <p className="modal-note">
            Settlement Generator has not changed or deleted anything. Your data and any backups stay
            in your user data folder.
          </p>
          <div className="button-row">
            <button type="button" className="btn" onClick={() => void exitApp()}>
              Exit
            </button>
          </div>
        </section>
      </div>
    </div>
  );
}

function Screens({ screen }: { screen: Screen }) {
  switch (screen) {
    case "menu":
      return <MainMenu />;
    case "settings":
      return <SettingsScreen />;
    case "roadmap":
      return <RoadmapScreen />;
    case "credits":
      return <CreditsScreen />;
    case "start":
      return <StartScreen />;
    case "view":
      return <ViewScreen />;
    case "tags":
      return <TagsScreen />;
    case "tables":
      return <TablesScreen />;
  }
}
