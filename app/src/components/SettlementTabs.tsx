import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";

export type SettlementTab = "details" | "map";

const TABS: { id: SettlementTab; label: string }[] = [
  { id: "details", label: "Settlement" },
  { id: "map", label: "Map" },
];

/**
 * Settlement / Map tabs for a settlement view. Left/Right (and Home/End)
 * move between tabs; the panel follows the selected tab.
 */
export function SettlementTabs({
  active,
  onChange,
  children,
}: {
  active: SettlementTab;
  onChange: (tab: SettlementTab) => void;
  children: ReactNode;
}) {
  const base = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = TABS.findIndex((t) => t.id === active);
    let next: number | null = null;
    if (event.key === "ArrowRight") next = (index + 1) % TABS.length;
    if (event.key === "ArrowLeft") next = (index - 1 + TABS.length) % TABS.length;
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = TABS.length - 1;
    if (next !== null) {
      event.preventDefault();
      onChange(TABS[next].id);
      listRef.current?.querySelectorAll<HTMLButtonElement>("[role=tab]")[next]?.focus();
    }
  };
  return (
    <>
      <div
        ref={listRef}
        className="settlement-tabs"
        role="tablist"
        aria-label="Settlement view"
        onKeyDown={onKey}
      >
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={`${base}-${t.id}-tab`}
            aria-selected={t.id === active}
            aria-controls={`${base}-${t.id}`}
            tabIndex={t.id === active ? 0 : -1}
            className={`settlement-tab ${t.id === active ? "is-active" : ""}`}
            onClick={() => onChange(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`${base}-${active}`}
        aria-labelledby={`${base}-${active}-tab`}
        className="settlement-tab-panel"
      >
        {children}
      </div>
    </>
  );
}
