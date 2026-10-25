// Generic building blocks for the content editors. Nothing here knows about
// any particular table or tag; everything is driven by the data model.
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  content,
  formatRange,
  framework,
  toPair,
  type Condition,
  type Named,
  type ContentOverview,
  type Coverage,
  type FaceRange,
  type TableDetail,
  type ValidationIssue,
} from "../../api";

// ----- catalogue of tables and tags (for pickers) -----------------------------

interface Catalog {
  overview: ContentOverview | null;
  refresh: () => Promise<ContentOverview | null>;
  /** Loads (and caches) a table's details, for result and variant pickers. */
  tableDetail: (id: string) => Promise<TableDetail | null>;
  tagName: (id: string) => string;
}

const CatalogContext = createContext<Catalog | null>(null);

export function CatalogProvider({ children }: { children: ReactNode }) {
  const [overview, setOverview] = useState<ContentOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const cache = useRef(new Map<string, Promise<TableDetail | null>>());

  const refresh = useCallback(async () => {
    cache.current.clear();
    try {
      const next = await content.overview();
      setOverview(next);
      setError(null);
      return next;
    } catch (e) {
      setError(`The content list could not be loaded: ${String(e)}`);
      return null;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const value = useMemo<Catalog>(
    () => ({
      overview,
      refresh,
      tableDetail: (id) => {
        let entry = cache.current.get(id);
        if (!entry) {
          // A failed load is not cached, so the next request tries again.
          entry = content.table(id).catch(() => {
            cache.current.delete(id);
            return null;
          });
          cache.current.set(id, entry);
        }
        return entry;
      },
      tagName: (id) => overview?.tags.find((t) => t.id === id)?.name ?? id,
    }),
    [overview, refresh],
  );
  return (
    <CatalogContext.Provider value={value}>
      {error && (
        <p role="alert" className="status-line is-error">
          {error}
        </p>
      )}
      {children}
    </CatalogContext.Provider>
  );
}

export function useCatalog(): Catalog {
  const ctx = useContext(CatalogContext);
  if (!ctx) throw new Error("useCatalog outside CatalogProvider");
  return ctx;
}

export function useTableDetail(id: string | undefined): TableDetail | null {
  const { tableDetail } = useCatalog();
  const [detail, setDetail] = useState<TableDetail | null>(null);
  useEffect(() => {
    let live = true;
    setDetail(null);
    if (id) void tableDetail(id).then((d) => live && setDetail(d));
    return () => {
      live = false;
    };
  }, [id, tableDetail]);
  return detail;
}

// ----- pickers ------------------------------------------------------------------

export function TagSelect({
  value,
  onChange,
  label,
  exclude = [],
}: {
  value: string;
  onChange: (id: string) => void;
  label: string;
  exclude?: string[];
}) {
  const { overview } = useCatalog();
  const known = overview?.tags.some((t) => t.id === value);
  return (
    <select
      className="select select-inline"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">Choose a tag…</option>
      {value && !known && <option value={value}>{value} (missing)</option>}
      {overview?.tags
        .filter((t) => !t.deleted && (t.id === value || !exclude.includes(t.id)))
        .map((t) => (
          <option key={t.id} value={t.id}>
            {t.name} — {t.id}
          </option>
        ))}
    </select>
  );
}

/** Selected tags as plain text with remove buttons, plus an add picker. */
export function TagMultiSelect({
  value,
  onChange,
  label,
}: {
  value: string[];
  onChange: (ids: string[]) => void;
  label: string;
}) {
  const { tagName } = useCatalog();
  return (
    <div className="multi">
      {value.length > 0 && (
        <ul className="multi-list" aria-label={label}>
          {value.map((id) => (
            <li key={id}>
              <span title={id}>{tagName(id)}</span>
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove ${tagName(id)}`}
                onClick={() => onChange(value.filter((v) => v !== id))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <TagSelect
        label={`Add to ${label}`}
        value=""
        exclude={value}
        onChange={(id) => id && onChange([...value, id])}
      />
    </div>
  );
}

export function TableSelect({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (id: string) => void;
  label: string;
}) {
  const { overview } = useCatalog();
  const known = overview?.tables.some((t) => t.id === value);
  return (
    <select
      className="select select-inline"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">Choose a table…</option>
      {value && !known && <option value={value}>{value} (not present)</option>}
      {overview?.tables
        .filter((t) => !t.deleted)
        .map((t) => (
          <option key={t.id} value={t.id}>
            {t.name} — {t.id}
          </option>
        ))}
    </select>
  );
}

/** Picks a flow (for effects that run one). */
export function FlowSelect({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (id: string) => void;
  label: string;
}) {
  const [flows, setFlows] = useState<Named[]>([]);
  useEffect(() => {
    framework.items().then(
      (items) =>
        setFlows(
          items
            .filter((i) => i.kind === "flow" && !i.deleted)
            .map((i) => ({ id: i.id, name: i.name })),
        ),
      () => setFlows([]),
    );
  }, []);
  const known = flows.some((f) => f.id === value);
  return (
    <select
      className="select select-inline"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">Choose a flow…</option>
      {value && !known && <option value={value}>{value} (not present)</option>}
      {flows.map((f) => (
        <option key={f.id} value={f.id}>
          {f.name}
        </option>
      ))}
    </select>
  );
}

// ----- conditions ---------------------------------------------------------------

type ConditionKind = "has_tag" | "all" | "any" | "not" | "setting_on";

function kindOf(condition: Condition): string {
  return Object.keys(condition)[0];
}

const KIND_LABELS: Record<ConditionKind, string> = {
  has_tag: "Has tag",
  all: "All of",
  any: "Any of",
  not: "Not",
  setting_on: "Setting is on",
};

function emptyOf(kind: ConditionKind): Condition {
  switch (kind) {
    case "has_tag":
      return { has_tag: "" };
    case "all":
      return { all: [{ has_tag: "" }] };
    case "any":
      return { any: [{ has_tag: "" }] };
    case "not":
      return { not: { has_tag: "" } };
    case "setting_on":
      return { setting_on: "" };
  }
}

/** Tests that only flows can hold, described as the core words them. */
function describeFlowTest(condition: Condition): string | undefined {
  if ("setting_is" in condition)
    return `Setting ${condition.setting_is.setting} is ${condition.setting_is.value}`;
  if ("result_is" in condition)
    return `Step ${condition.result_is.step} gave ${condition.result_is.results.join(" or ")}`;
  if ("value_in" in condition) {
    const { step, min, max } = condition.value_in;
    return `Step ${step}'s number is ${min ?? "any"} to ${max ?? "any"}`;
  }
  return undefined;
}

/** A picker over the on/off settings a condition can test. */
function SettingSelect({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (id: string) => void;
  label: string;
}) {
  const [settings, setSettings] = useState<Named[]>([]);
  useEffect(() => {
    framework
      .toggleSettings()
      .then(setSettings)
      .catch(() => setSettings([]));
  }, []);
  return (
    <select
      className="select select-inline"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">Choose a setting…</option>
      {settings.map((s) => (
        <option key={s.id} value={s.id}>
          {s.name}
        </option>
      ))}
      {value && !settings.some((s) => s.id === value) && <option value={value}>{value}</option>}
    </select>
  );
}

/** Optional condition: "Always" until one is added. */
export function OptionalCondition({
  value,
  onChange,
  label,
  emptyLabel = "Always",
}: {
  value: Condition | undefined;
  onChange: (value: Condition | undefined) => void;
  label: string;
  emptyLabel?: string;
}) {
  if (!value) {
    return (
      <div className="condition-empty">
        <span className="muted">{emptyLabel}</span>
        <button type="button" className="btn btn-small" onClick={() => onChange({ has_tag: "" })}>
          Add condition<span className="visually-hidden"> to {label}</span>
        </button>
      </div>
    );
  }
  return (
    <div className="condition-root">
      <ConditionEditor value={value} onChange={onChange} label={label} />
      <button type="button" className="btn btn-small" onClick={() => onChange(undefined)}>
        Remove condition<span className="visually-hidden"> from {label}</span>
      </button>
    </div>
  );
}

/** Recursive condition builder over tags. */
export function ConditionEditor({
  value,
  onChange,
  label,
}: {
  value: Condition;
  onChange: (value: Condition) => void;
  label: string;
}) {
  const kind = kindOf(value);
  const flowTest = describeFlowTest(value);
  return (
    <div className="condition">
      <select
        className="select select-inline"
        aria-label={`${label}: condition type`}
        value={kind}
        onChange={(e) => onChange(emptyOf(e.target.value as ConditionKind))}
      >
        {(Object.keys(KIND_LABELS) as ConditionKind[]).map((k) => (
          <option key={k} value={k}>
            {KIND_LABELS[k]}
          </option>
        ))}
        {flowTest && <option value={kind}>Flow test</option>}
      </select>
      {flowTest && <span className="muted">{flowTest}</span>}
      {"setting_on" in value && (
        <SettingSelect
          label={`${label}: setting`}
          value={value.setting_on}
          onChange={(id) => onChange({ setting_on: id })}
        />
      )}
      {"has_tag" in value && (
        <TagSelect
          label={`${label}: tag`}
          value={value.has_tag}
          onChange={(id) => onChange({ has_tag: id })}
        />
      )}
      {"not" in value && (
        <div className="condition-children">
          <ConditionEditor
            value={value.not}
            label={`${label} (not)`}
            onChange={(c) => onChange({ not: c })}
          />
        </div>
      )}
      {("all" in value || "any" in value) && (
        <ConditionList
          items={"all" in value ? value.all : value.any}
          label={label}
          onChange={(items) => onChange("all" in value ? { all: items } : { any: items })}
        />
      )}
    </div>
  );
}

function ConditionList({
  items,
  label,
  onChange,
}: {
  items: Condition[];
  label: string;
  onChange: (items: Condition[]) => void;
}) {
  return (
    <div className="condition-children">
      {items.map((item, i) => (
        <div key={i} className="condition-item">
          <ConditionEditor
            value={item}
            label={`${label} ${i + 1}`}
            onChange={(c) => onChange(items.map((x, j) => (j === i ? c : x)))}
          />
          <button
            type="button"
            className="icon-button"
            aria-label={`Remove ${label} ${i + 1}`}
            onClick={() => onChange(items.filter((_, j) => j !== i))}
          >
            ×
          </button>
        </div>
      ))}
      <button
        type="button"
        className="btn btn-small"
        onClick={() => onChange([...items, { has_tag: "" }])}
      >
        Add<span className="visually-hidden"> to {label}</span>
      </button>
    </div>
  );
}

// ----- faces and coverage ------------------------------------------------------------

/** Two number boxes for a range of faces; empty means "no faces". */
export function RangeInput({
  value,
  onChange,
  label,
  optional = false,
}: {
  value: FaceRange | undefined;
  onChange: (value: FaceRange | undefined) => void;
  label: string;
  optional?: boolean;
}) {
  const [min, max] = value === undefined ? ["", ""] : toPair(value).map(String);
  const update = (nextMin: string, nextMax: string) => {
    if (optional && nextMin === "" && nextMax === "") return onChange(undefined);
    const a = Number.parseInt(nextMin || nextMax, 10);
    const b = Number.parseInt(nextMax || nextMin, 10);
    if (Number.isNaN(a) || Number.isNaN(b)) return;
    onChange(a === b ? a : [a, b]);
  };
  return (
    <span className="range-input">
      <input
        type="number"
        className="num-input"
        aria-label={`${label}: from`}
        value={min}
        onChange={(e) => update(e.target.value, max)}
      />
      <span aria-hidden="true">–</span>
      <input
        type="number"
        className="num-input"
        aria-label={`${label}: to`}
        value={max}
        onChange={(e) => update(min, e.target.value)}
      />
    </span>
  );
}

const percent = (p: number) =>
  p === 0 ? "0%" : p < 0.001 ? "<0.1%" : `${(p * 100).toFixed(p < 0.1 ? 1 : 0)}%`;

/**
 * Every total the dice can roll, coloured by who claims it: uncovered and
 * overlapping faces stand out so physical-dice validity is obvious.
 */
export function CoverageStrip({
  coverage,
  nameOf,
}: {
  coverage: Coverage;
  nameOf: (id: string) => string;
}) {
  const ok =
    !coverage.dice_error &&
    coverage.uncovered.length === 0 &&
    coverage.overlapping.length === 0 &&
    coverage.impossible.length === 0;
  return (
    <figure className={`coverage ${ok ? "is-valid" : "is-invalid"}`} aria-label={coverage.label}>
      <figcaption className="coverage-caption">
        <span className="label">{coverage.label}</span>
        <span className="coverage-dice">{coverage.dice}</span>
        {coverage.dice_error ? (
          <span className="coverage-problem">{coverage.dice_error}</span>
        ) : ok ? (
          <span className="coverage-ok">Every face is covered exactly once</span>
        ) : (
          <span className="coverage-problem">
            {coverage.uncovered.length > 0 &&
              `Uncovered: ${coverage.uncovered.map(formatRange).join(", ")}. `}
            {coverage.overlapping.length > 0 &&
              `Overlapping: ${coverage.overlapping.map(formatRange).join(", ")}. `}
            {coverage.impossible.map(
              (r) => `${nameOf(r.result)} (${formatRange(r.range)}) cannot be rolled. `,
            )}
          </span>
        )}
      </figcaption>
      {coverage.faces.length > 0 && (
        <ol className="faces">
          {coverage.faces.map((f) => {
            const state =
              f.claimed_by.length === 0 ? "uncovered" : f.claimed_by.length > 1 ? "overlap" : "ok";
            const who = f.claimed_by.map(nameOf).join(", ") || "no result";
            return (
              <li
                key={f.face}
                className={`face face-${state}`}
                title={`${f.face}: ${who} (${percent(f.chance)})`}
              >
                {f.face}
              </li>
            );
          })}
        </ol>
      )}
    </figure>
  );
}

/** Chance of a result in a coverage map, e.g. "15%". */
export function chanceOf(coverage: Coverage | undefined, resultId: string): string {
  const share = coverage?.results.find((r) => r.result === resultId);
  return share ? percent(share.chance) : "";
}

// ----- issues -------------------------------------------------------------------

export function IssueList({ issues, error }: { issues: ValidationIssue[]; error?: string | null }) {
  if (!error && issues.length === 0) {
    return <p className="issues-ok">No problems found.</p>;
  }
  return (
    <ul className="issues" aria-label="Problems">
      {error && <li className="issue issue-error">{error}</li>}
      {issues.map((issue, i) => (
        <li key={i} className={`issue issue-${issue.severity}`}>
          <span className="issue-level">{issue.severity === "error" ? "Error" : "Warning"}</span>{" "}
          {issue.message}
        </li>
      ))}
    </ul>
  );
}

/** Calls `check` with the latest draft, debounced, and keeps the result. */
export function useDraftCheck<T, R>(
  draft: T | null,
  check: (draft: T) => Promise<R>,
  delay = 250,
): R | null {
  const [result, setResult] = useState<R | null>(null);
  useEffect(() => {
    if (draft === null) return;
    let live = true;
    const timer = setTimeout(() => {
      check(draft).then(
        (r) => live && setResult(r),
        () => undefined,
      );
    }, delay);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [draft, check, delay]);
  return result;
}

export function originLabel(origin: string | null, deleted: boolean): string {
  if (deleted) return "Deleted default";
  switch (origin) {
    case "bundled":
      return "Default";
    case "modified":
      return "Customised default";
    case "custom":
      return "Custom";
    default:
      return "";
  }
}
