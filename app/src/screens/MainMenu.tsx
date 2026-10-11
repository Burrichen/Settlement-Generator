import { useEffect, useRef, type KeyboardEvent } from "react";
import { displayVersion, exitApp } from "../api";
import { useApp, type Screen } from "../app-state";
import { Ornament } from "../components/Ornament";

type Item =
  | { label: string; to: Screen }
  | { label: string; action: () => void }
  | { label: string; disabled: true };

export function MainMenu() {
  const { info, navigate, menuFocus } = useApp();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);

  const items: Item[] = [
    { label: "Start", to: "start" },
    { label: "Settings", to: "settings" },
    { label: "View", to: "view" },
    { label: "Roadmap", to: "roadmap" },
    { label: "DLC Content", disabled: true },
    { label: "Exit", action: () => void exitApp() },
  ];

  useEffect(() => {
    buttons.current[menuFocus.current]?.focus();
  }, [menuFocus]);

  // Arrow keys move between the enabled items only.
  const focusables = () => buttons.current.filter((b): b is HTMLButtonElement => b !== null);
  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const list = focusables();
    const index = list.indexOf(document.activeElement as HTMLButtonElement);
    let next: number | null = null;
    if (event.key === "ArrowDown") next = (index + 1) % list.length;
    if (event.key === "ArrowUp") next = (index - 1 + list.length) % list.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = list.length - 1;
    if (next !== null) {
      event.preventDefault();
      list[next].focus();
    }
  };

  return (
    <div className="menu-screen">
      <div className="menu-center">
        <nav className="menu-panel" aria-label="Main menu">
          <h1 className="display menu-title">{info?.name ?? "Settlement Generator"}</h1>
          <Ornament />
          <ul className="menu-list" onKeyDown={onKeyDown}>
            {items.map((item, i) => (
              <li key={item.label}>
                {"disabled" in item ? (
                  <span className="menu-item is-disabled" aria-disabled="true">
                    {item.label}
                  </span>
                ) : (
                  <button
                    type="button"
                    className="menu-item"
                    ref={(el) => {
                      buttons.current[i] = el;
                    }}
                    onFocus={() => {
                      menuFocus.current = i;
                    }}
                    onClick={() => ("to" in item ? navigate(item.to) : item.action())}
                  >
                    {item.label}
                    <span className="menu-marker" aria-hidden="true">
                      ›
                    </span>
                  </button>
                )}
              </li>
            ))}
          </ul>
        </nav>
      </div>
      <footer className="menu-footer">
        <span>↑ ↓ to move · Enter to select</span>
        <span>{info ? displayVersion(info) : ""}</span>
      </footer>
    </div>
  );
}
