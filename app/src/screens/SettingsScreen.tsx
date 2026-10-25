import { useEffect, useState, type ReactNode } from "react";
import {
  displayVersion,
  framework,
  type ContentSettingView,
  type SettingValue,
  type GenerationMethod,
  type GenerationPresentation,
  type Settings,
  type Theme,
} from "../api";
import { useApp } from "../app-state";
import { ScreenFrame } from "../components/ScreenFrame";

/** Settings save immediately on every change. */
export function SettingsScreen() {
  const { info, settings, updateSettings, navigate, back } = useApp();
  const [status, setStatus] = useState<{ text: string; error: boolean }>({
    text: "",
    error: false,
  });

  const change = async (patch: Partial<Settings>) => {
    if (!settings) return;
    const error = await updateSettings({ ...settings, ...patch });
    setStatus(error ? { text: error, error: true } : { text: "Saved", error: false });
  };

  const automatic = settings?.generation_method === "automatic";

  return (
    <ScreenFrame
      title="Settings"
      subtitle="Changes are saved as soon as you make them."
      backLabel="Main Menu"
      onBack={back}
    >
      {!settings ? (
        <p className="muted">Loading settings…</p>
      ) : (
        <>
          <section className="settings-section" aria-labelledby="sec-generation">
            <span id="sec-generation" className="label">
              Generation
            </span>
            <div className="settings-group">
              <ChoiceRow<GenerationMethod>
                name="generation-method"
                title="Generation Method"
                help="Automatic resolves every stage at once. Manual lets you roll or choose each stage."
                value={settings.generation_method}
                options={[
                  { value: "automatic", label: "Automatic" },
                  { value: "manual", label: "Manual" },
                ]}
                onChange={(v) => change({ generation_method: v })}
              />
              <ChoiceRow<GenerationPresentation>
                name="generation-presentation"
                title="Generation Presentation"
                help={
                  automatic
                    ? "Only used with Manual generation. Your choice is kept for when you switch back."
                    : "Incremental shows one stage at a time. One-table shows every stage together."
                }
                value={settings.generation_presentation}
                disabled={automatic}
                options={[
                  { value: "incremental", label: "Incremental" },
                  { value: "one_table", label: "One-table" },
                ]}
                onChange={(v) => change({ generation_presentation: v })}
              />
              <ToggleRow
                id="god-mode"
                title="God Mode"
                help="Every result of a table can be chosen, ignoring tag-based limits. Chosen results still apply their tags."
                checked={settings.god_mode}
                onChange={(v) => change({ god_mode: v })}
              />
              <ToggleRow
                id="roll-details"
                title="Roll details (advanced)"
                help="Show how every generated value was reached: each roll, modifier and scale move."
                checked={settings.roll_details ?? false}
                onChange={(v) => change({ roll_details: v })}
              />
              <ToggleRow
                id="tag-view"
                title="Tag View"
                help="Show applied tags during generation and when viewing saved settlements."
                checked={settings.tag_view}
                onChange={(v) => change({ tag_view: v })}
              />
              <ContentSettings onStatus={setStatus} />
            </div>
          </section>

          <section className="settings-section" aria-labelledby="sec-content">
            <span id="sec-content" className="label">
              Content
            </span>
            <div className="settings-group">
              <NavRow
                title="View Tags"
                help="Browse and customise tags."
                onOpen={() => navigate("tags")}
              />
              <NavRow
                title="View Tables"
                help="Browse and customise tables."
                onOpen={() => navigate("tables")}
              />
              <NavRow
                title="View Generation Settings"
                help="Create and change the settings generation offers, and see what uses them."
                onOpen={() => navigate("setting-defs")}
              />
              <NavRow
                title="Import / Export"
                help="Share tables, tags, settings and flows as files, or back up everything you made."
                onOpen={() => navigate("transfer")}
              />
              <NavRow
                title="View Flows"
                help="See and change how generation runs, step by step, as a diagram."
                onOpen={() => navigate("flows")}
              />
            </div>
          </section>

          <section className="settings-section" aria-labelledby="sec-appearance">
            <span id="sec-appearance" className="label">
              Appearance
            </span>
            <div className="settings-group">
              <ChoiceRow<Theme>
                name="theme"
                title="Theme"
                value={settings.theme}
                options={[
                  { value: "modern_dark", label: "Modern Dark" },
                  { value: "old_timey_dark", label: "Old-Timey Dark" },
                ]}
                onChange={(v) => change({ theme: v })}
              />
            </div>
          </section>

          <section className="settings-section" aria-labelledby="sec-about">
            <span id="sec-about" className="label">
              About
            </span>
            <div className="settings-group">
              <Row title="Version">
                <span className="setting-value">{info ? displayVersion(info) : "…"}</span>
              </Row>
              <NavRow title="Credits" onOpen={() => navigate("credits")} />
            </div>
          </section>

          <p
            className={`status-line ${status.error ? "is-error" : ""}`}
            role="status"
            aria-live="polite"
          >
            {status.text}
          </p>
        </>
      )}
    </ScreenFrame>
  );
}

/**
 * Settings the generator content defines for itself (none in the bundled
 * content), each a toggle or a choice, saved through the core.
 */
function ContentSettings({
  onStatus,
}: {
  onStatus: (status: { text: string; error: boolean }) => void;
}) {
  const [list, setList] = useState<ContentSettingView[]>([]);
  useEffect(() => {
    framework
      .settings()
      .then(setList)
      .catch(() => setList([]));
  }, []);
  const set = async (id: string, value: SettingValue) => {
    try {
      setList(await framework.setSetting(id, value));
      onStatus({ text: "Saved", error: false });
    } catch (e) {
      onStatus({ text: String(e), error: true });
    }
  };
  return (
    <>
      {list
        // Planned settings are documented, not offered.
        .filter(({ setting }) => setting.status !== "planned")
        .map(({ setting, value }) => (
          <ContentSettingRow
            key={setting.id}
            setting={setting}
            value={value}
            onChange={(v) => void set(setting.id, v)}
          />
        ))}
    </>
  );
}

function ContentSettingRow({
  setting,
  value,
  onChange,
}: {
  setting: ContentSettingView["setting"];
  value: SettingValue;
  onChange: (value: SettingValue) => void;
}) {
  const [text, setText] = useState(typeof value === "string" ? value : "");
  useEffect(() => setText(typeof value === "string" ? value : ""), [value]);
  switch (setting.kind) {
    case "choice":
      return (
        <ChoiceRow<string>
          name={`setting-${setting.id}`}
          title={setting.name}
          help={setting.description}
          value={String(value)}
          options={setting.options.map((o) => ({ value: o.id, label: o.name }))}
          onChange={onChange}
        />
      );
    case "toggle":
      return (
        <ToggleRow
          id={`setting-${setting.id}`}
          title={setting.name}
          help={setting.description ?? ""}
          checked={value === true}
          onChange={onChange}
        />
      );
    case "number":
      return (
        <Row title={setting.name} help={setting.description}>
          <input
            className="num-input"
            type="number"
            aria-label={setting.name}
            min={setting.min}
            max={setting.max}
            value={typeof value === "number" ? value : ""}
            onChange={(e) => {
              const n = Number.parseInt(e.target.value, 10);
              if (!Number.isNaN(n)) onChange(n);
            }}
          />
        </Row>
      );
    case "multi_select": {
      const ticked = Array.isArray(value) ? value : [];
      return (
        <Row title={setting.name} help={setting.description}>
          <span className="check-list" role="group" aria-label={setting.name}>
            {setting.options.map((o) => (
              <label key={o.id} className="check">
                <input
                  type="checkbox"
                  checked={ticked.includes(o.id)}
                  onChange={(e) =>
                    onChange(
                      e.target.checked ? [...ticked, o.id] : ticked.filter((id) => id !== o.id),
                    )
                  }
                />
                {o.name}
              </label>
            ))}
          </span>
        </Row>
      );
    }
    case "text":
      return (
        <Row title={setting.name} help={setting.description}>
          <input
            className="text-input"
            aria-label={setting.name}
            maxLength={200}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={() => text !== value && onChange(text)}
          />
        </Row>
      );
  }
}

function Row({ title, help, children }: { title: string; help?: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div>
        <span className="setting-name">{title}</span>
        {help && <span className="setting-help">{help}</span>}
      </div>
      {children}
    </div>
  );
}

interface ChoiceProps<T extends string> {
  name: string;
  title: string;
  help?: string;
  value: T;
  options: { value: T; label: string }[];
  disabled?: boolean;
  onChange: (value: T) => void;
}

/** A radio group: arrow keys move between options, as native radios do. */
function ChoiceRow<T extends string>({
  name,
  title,
  help,
  value,
  options,
  disabled = false,
  onChange,
}: ChoiceProps<T>) {
  return (
    <div className={`setting-row ${disabled ? "is-disabled" : ""}`}>
      <div>
        <span id={`${name}-label`} className="setting-name">
          {title}
        </span>
        {help && (
          <span id={`${name}-help`} className="setting-help">
            {help}
          </span>
        )}
      </div>
      <div
        role="radiogroup"
        className="segmented"
        aria-labelledby={`${name}-label`}
        aria-describedby={help ? `${name}-help` : undefined}
        aria-disabled={disabled || undefined}
      >
        {options.map((option) => (
          <label key={option.value}>
            <input
              type="radio"
              name={name}
              value={option.value}
              checked={value === option.value}
              disabled={disabled}
              onChange={() => onChange(option.value)}
            />
            <span>{option.label}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

function ToggleRow({
  id,
  title,
  help,
  checked,
  onChange,
}: {
  id: string;
  title: string;
  help: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="setting-row">
      <div>
        <label htmlFor={id} className="setting-name">
          {title}
        </label>
        <span className="setting-help" id={`${id}-help`}>
          {help}
        </span>
      </div>
      <span className="toggle">
        <span className="toggle-state" aria-hidden="true">
          {checked ? "On" : "Off"}
        </span>
        <input
          id={id}
          type="checkbox"
          role="switch"
          aria-describedby={`${id}-help`}
          checked={checked}
          onChange={(e) => onChange(e.target.checked)}
        />
      </span>
    </div>
  );
}

function NavRow({ title, help, onOpen }: { title: string; help?: string; onOpen: () => void }) {
  return (
    <Row title={title} help={help}>
      <button type="button" className="nav-button" onClick={onOpen}>
        Open{" "}
        <span className="chev" aria-hidden="true">
          ›
        </span>
        <span className="visually-hidden">{title}</span>
      </button>
    </Row>
  );
}
