import { useEffect, useRef, type ReactNode } from "react";

interface Props {
  title: string;
  subtitle?: string;
  backLabel: string;
  onBack: () => void;
  children: ReactNode;
  /** Use the full window width (for two-pane screens). */
  wide?: boolean;
}

/** Standard sub-screen: back control, title, ornament, body, key hints. */
export function ScreenFrame({ title, subtitle, backLabel, onBack, children, wide }: Props) {
  const heading = useRef<HTMLHeadingElement>(null);

  // Move focus into the new screen so keyboard users start at its top.
  useEffect(() => {
    heading.current?.focus();
  }, []);

  return (
    <div className={`screen ${wide ? "screen-wide" : ""}`}>
      <header className="screen-header">
        <div className="screen-header-inner">
          <button type="button" className="back-button" onClick={onBack}>
            <span aria-hidden="true">‹</span> {backLabel}
          </button>
          <h1 ref={heading} tabIndex={-1} className="display screen-title">
            {title}
          </h1>
          {subtitle && <p className="screen-subtitle">{subtitle}</p>}
        </div>
      </header>
      <main className="screen-body">{children}</main>
      <KeyHints />
    </div>
  );
}

export function KeyHints() {
  return (
    <footer className="kbd-hint">
      <kbd>Tab</kbd> move · <kbd>Enter</kbd> select · <kbd>Esc</kbd> back
    </footer>
  );
}
