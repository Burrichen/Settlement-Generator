// Test and preview for any table or flow: Roll once, Step through (one line
// at a time), and Run 100 / 1,000 / 10,000 with what came up compared to
// what the dice should give. Simple by default: one row of buttons; the
// results and the advanced options appear only when asked for.
import { useState } from "react";
import {
  content,
  flows,
  type ErrorCount,
  type Flow,
  type FlowPreview,
  type FlowSimulation,
  type ModifierUse,
  type OutcomeStat,
  type RollTrace,
  type Table,
  type TableSimulation,
  type ValidationIssue,
} from "../../api";
import { IssueList, TagMultiSelect } from "./common";

const RUNS = [100, 1000, 10000];

type Target = { kind: "table"; table: Table } | { kind: "flow"; flow: Flow };

type Shown =
  | { mode: "once"; lines: Line[]; error?: string }
  | { mode: "step"; lines: Line[]; at: number; error?: string }
  | { mode: "table-runs"; sim: TableSimulation }
  | { mode: "flow-runs"; sim: FlowSimulation };

/** One line of a roll or run, for Roll once and Step through. */
interface Line {
  depth: number;
  text: string;
  strong?: boolean;
}

function traceLines(traces: RollTrace[]): Line[] {
  return traces.flatMap((t) => [
    { depth: 0, text: t.title, strong: true },
    ...t.lines.map((text) => ({ depth: 1, text })),
  ]);
}

function previewLines(preview: FlowPreview): Line[] {
  return [
    ...preview.outline.map((l) => ({
      depth: l.depth,
      text: l.value ? `${l.label}: ${l.value}` : l.label,
    })),
    ...(preview.traces.length > 0
      ? [
          { depth: 0, text: "How each roll was reached", strong: true },
          ...traceLines(preview.traces),
        ]
      : []),
  ];
}

const fmt = (n: number) => `${n.toFixed(1)}%`;

export function TestPanel({ target, disabled }: { target: Target; disabled: boolean }) {
  const [shown, setShown] = useState<Shown | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [tags, setTags] = useState<string[]>([]);
  const noun = target.kind === "table" ? "table" : "flow";

  const once = async (): Promise<{ lines: Line[]; error?: string }> => {
    if (target.kind === "table") {
      const preview = await content.previewRoll(target.table);
      return { lines: traceLines(preview.traces) };
    }
    const preview = await flows.preview(target.flow);
    return { lines: previewLines(preview), error: preview.error };
  };

  const run = async (label: string, job: () => Promise<void>) => {
    setBusy(label);
    setError(null);
    try {
      await job();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="editor-section test-panel" aria-labelledby={`test-${noun}`}>
      <h3 id={`test-${noun}`} className="label">
        Test this {noun}
      </h3>
      <p className="muted small">
        Try it with software dice. Nothing is saved, and real generation still uses your dice.
      </p>
      <div className="toolbar" role="toolbar" aria-label={`Test this ${noun}`}>
        <button
          type="button"
          className="btn btn-small"
          disabled={disabled || busy !== null}
          onClick={() =>
            void run("once", async () => {
              const r = await once();
              setShown({ mode: "once", ...r });
            })
          }
        >
          Roll once
        </button>
        <button
          type="button"
          className="btn btn-small"
          disabled={disabled || busy !== null}
          onClick={() =>
            void run("step", async () => {
              const r = await once();
              setShown({ mode: "step", at: 1, ...r });
            })
          }
        >
          Step through
        </button>
        {RUNS.map((n) => (
          <button
            key={n}
            type="button"
            className="btn btn-small"
            disabled={disabled || busy !== null}
            onClick={() =>
              void run(`runs-${n}`, async () => {
                if (target.kind === "table") {
                  const sim = await content.simulateTable(target.table, n, tags);
                  setShown({ mode: "table-runs", sim });
                } else {
                  const sim = await flows.simulate(target.flow, n);
                  setShown({ mode: "flow-runs", sim });
                }
              })
            }
          >
            Run {n.toLocaleString("en-GB")}
          </button>
        ))}
        {target.kind === "table" && (
          <button
            type="button"
            className="btn btn-small"
            aria-expanded={advanced}
            onClick={() => setAdvanced(!advanced)}
          >
            {advanced ? "Hide options" : "Options…"}
          </button>
        )}
      </div>
      {disabled && <p className="muted small">Fix the problems listed under Validation to test.</p>}
      {advanced && target.kind === "table" && (
        <div className="field">
          <span className="field-label">Test with these tags active</span>
          <TagMultiSelect value={tags} onChange={setTags} label="Tags active while testing" />
        </div>
      )}
      {busy && (
        <p role="status" className="muted">
          {busy.startsWith("runs") ? "Running… large runs can take a few seconds." : "Rolling…"}
        </p>
      )}
      {error && (
        <p role="alert" className="status-line is-error">
          {error}
        </p>
      )}
      {shown && <Results shown={shown} setShown={setShown} />}
    </section>
  );
}

function Results({ shown, setShown }: { shown: Shown; setShown: (s: Shown | null) => void }) {
  const close = (
    <button type="button" className="btn btn-small" onClick={() => setShown(null)}>
      Close
    </button>
  );
  switch (shown.mode) {
    case "once":
    case "step": {
      const lines = shown.mode === "step" ? shown.lines.slice(0, shown.at) : shown.lines;
      const done = shown.mode === "once" || shown.at >= shown.lines.length;
      return (
        <div className="test-results">
          {shown.error && <p className="status-line is-error">{shown.error}</p>}
          <ol className="trace" aria-label={shown.mode === "step" ? "Step through" : "Roll once"}>
            {lines.map((l, i) => (
              <li
                key={i}
                className={l.strong ? "trace-title" : "trace-line"}
                style={{ paddingLeft: `${l.depth * 1.25}em` }}
              >
                {l.text}
              </li>
            ))}
          </ol>
          <div className="button-row">
            {shown.mode === "step" && !done && (
              <button
                type="button"
                className="btn btn-small btn-primary"
                onClick={() => setShown({ ...shown, at: shown.at + 1 })}
              >
                Next step
              </button>
            )}
            {shown.mode === "step" && !done && (
              <button
                type="button"
                className="btn btn-small"
                onClick={() => setShown({ ...shown, at: shown.lines.length })}
              >
                Show all
              </button>
            )}
            {close}
          </div>
        </div>
      );
    }
    case "table-runs": {
      const { sim } = shown;
      return (
        <div className="test-results">
          <OutcomeTable
            caption={`${sim.table.name}: ${sim.runs.toLocaleString("en-GB")} rolls`}
            rows={sim.results}
            expected
          />
          {sim.unreachable.length > 0 && (
            <p className="note" role="note">
              Never possible now: {sim.unreachable.map((r) => r.name).join(", ")}.
            </p>
          )}
          {sim.rerolls > 0 && (
            <p className="muted small">
              On average {sim.rerolls.toFixed(2)} roll(s) were made again per result (results that
              could not be had).
            </p>
          )}
          <Usage modifiers={sim.modifiers} errors={sim.errors} issues={sim.issues} />
          <p className="muted small">{sim.note}</p>
          {close}
        </div>
      );
    }
    case "flow-runs": {
      const { sim } = shown;
      return (
        <div className="test-results">
          {sim.steps.map((s) => (
            <OutcomeTable
              key={s.step}
              caption={`${s.label} (ran in ${s.reached.toLocaleString("en-GB")} of ${sim.runs.toLocaleString("en-GB")} runs)`}
              rows={s.outcomes.slice(0, 12)}
              more={Math.max(0, s.outcomes.length - 12)}
            />
          ))}
          {sim.branches.map((b) => (
            <OutcomeTable key={b.step} caption={`Condition “${b.label}”`} rows={b.cases} />
          ))}
          {sim.impossible.length > 0 && (
            <div className="note" role="note">
              <p className="field-label">Branches that never ran</p>
              <ul>
                {sim.impossible.map((m, i) => (
                  <li key={i}>{m}</li>
                ))}
              </ul>
            </div>
          )}
          <Usage modifiers={sim.modifiers} errors={sim.errors} issues={sim.issues} />
          <p className="muted small">{sim.note}</p>
          {close}
        </div>
      );
    }
  }
}

function OutcomeTable({
  caption,
  rows,
  expected = false,
  more = 0,
}: {
  caption: string;
  rows: OutcomeStat[];
  expected?: boolean;
  more?: number;
}) {
  return (
    <table className="edit-grid sim-table">
      <caption>{caption}</caption>
      <thead>
        <tr>
          <th scope="col">Outcome</th>
          {expected && <th scope="col">Expected</th>}
          <th scope="col">Actual</th>
          <th scope="col">Count</th>
          <th scope="col">
            <span className="sr-only">Share</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => {
          const off = expected && r.expected !== undefined && Math.abs(r.actual - r.expected) >= 5;
          return (
            <tr key={r.id} className={r.count === 0 ? "is-muted" : ""}>
              <th scope="row">{r.name}</th>
              {expected && <td>{r.expected === undefined ? "—" : fmt(r.expected)}</td>}
              <td className={off ? "sim-off" : ""}>{fmt(r.actual)}</td>
              <td>{r.count.toLocaleString("en-GB")}</td>
              <td className="sim-bar-cell" aria-hidden="true">
                <span className="sim-bar" style={{ width: `${Math.min(100, r.actual)}%` }} />
                {expected && r.expected !== undefined && (
                  <span
                    className="sim-expected"
                    style={{ left: `${Math.min(100, r.expected)}%` }}
                  />
                )}
              </td>
            </tr>
          );
        })}
        {more > 0 && (
          <tr>
            <td colSpan={expected ? 5 : 4} className="muted small">
              …and {more} more
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

function Usage({
  modifiers,
  errors,
  issues,
}: {
  modifiers: ModifierUse[];
  errors: ErrorCount[];
  issues: ValidationIssue[];
}) {
  return (
    <>
      {modifiers.length > 0 && (
        <div>
          <p className="field-label">Modifiers used</p>
          <ul className="usage-list" aria-label="Modifiers used">
            {modifiers.map((m) => (
              <li key={m.label}>
                {m.label}: {m.count.toLocaleString("en-GB")} time{m.count === 1 ? "" : "s"}
              </li>
            ))}
          </ul>
        </div>
      )}
      {errors.length > 0 && (
        <div role="alert">
          <p className="field-label">Runs that stopped with an error</p>
          <ul className="usage-list">
            {errors.map((e) => (
              <li key={e.message} className="issue issue-error">
                {e.count.toLocaleString("en-GB")} × {e.message}
              </li>
            ))}
          </ul>
        </div>
      )}
      {issues.length > 0 && <IssueList issues={issues} />}
    </>
  );
}
