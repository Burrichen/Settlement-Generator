// Editors for naming content: the approved names a result holds for a
// `curated_variant` naming method, and each naming method's joining rule and
// spelling exceptions. Both are driven by what the core reports about the
// table (TableDetail), so nothing here is specific to a region.
import { useEffect, useState } from "react";
import {
  content,
  type Capitalisation,
  type DraftCheck,
  type JoinRule,
  type Named,
  type NamingMethodDetail,
  type SpellingException,
  type TableDetail,
  type TableResult,
} from "../../api";
import { IssueList, useDraftCheck } from "./common";

// ----- approved names -------------------------------------------------------------

/**
 * One input per variant the naming data needs (e.g. A and B), each edited
 * on its own. An emptied input removes that name, which validation then
 * reports. Names keyed by an ID that is not a variant are listed so they
 * can be removed.
 */
export function ApprovedNamesEditor({
  result,
  variants,
  onChange,
}: {
  result: TableResult;
  variants: Named[];
  onChange: (approved: Record<string, string>) => void;
}) {
  const names = result.approved_names ?? {};
  const label = result.name || result.id;
  const set = (key: string, value: string) => {
    const next = { ...names };
    if (value.trim() === "") delete next[key];
    else next[key] = value;
    onChange(next);
  };
  const strays = Object.keys(names).filter((k) => !variants.some((v) => v.id === k));
  return (
    <div className="approved-names">
      <span className="field-label">Approved names</span>
      <span className="field-help"> Used exactly as written; nothing is added.</span>
      <div className="approved-names-grid">
        {variants.map((v) => {
          const value = names[v.id] ?? "";
          const missing = value.trim() === "";
          return (
            <label key={v.id} className="field approved-name">
              <span className="field-label">
                Variant {v.name}
                {missing && <span className="field-error"> — missing</span>}
              </span>
              <input
                className="text-input"
                aria-label={`${label}: approved name ${v.name}`}
                aria-invalid={missing || undefined}
                value={value}
                onChange={(e) => set(v.id, e.target.value)}
              />
            </label>
          );
        })}
        {strays.map((key) => (
          <span key={key} className="field approved-name">
            <span className="field-label">
              <code>{key}</code> (not a variant)
            </span>
            <span className="variant-row">
              <input
                className="text-input"
                aria-label={`${label}: approved name ${key}`}
                value={names[key]}
                onChange={(e) => set(key, e.target.value)}
              />
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove approved name ${key}`}
                onClick={() => set(key, "")}
              >
                ×
              </button>
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}

/** Every variant the table's results need approved names for, in order. */
export function approvedVariants(detail: TableDetail | null): Named[] {
  const out: Named[] = [];
  for (const slots of detail?.approved_names ?? []) {
    for (const v of slots.variants) {
      if (!out.some((o) => o.id === v.id)) out.push(v);
    }
  }
  return out;
}

// ----- naming methods ---------------------------------------------------------------

const STYLES: { value: JoinRule["style"]; label: string }[] = [
  { value: "compound", label: "Compound — parts joined directly (Alderwick)" },
  { value: "space", label: "Space — one space, words capitalised (Khar Well)" },
  { value: "possessive", label: "Possessive — a suffix before endings (Godwinsbury)" },
  {
    value: "curated_variant",
    label: "Curated variant — an approved name chosen by the variant (Oswaldwick)",
  },
];

const CASES: { value: Capitalisation; label: string }[] = [
  { value: "sentence", label: "Sentence (Alderwick)" },
  { value: "words", label: "Each word (Khar Well)" },
  { value: "as_written", label: "As written" },
];

/** A join rule with `style` changed, keeping the shared settings. */
function withStyle(join: JoinRule, style: JoinRule["style"]): JoinRule {
  if (join.style === style) return join;
  const shared = {
    separator: join.separator,
    case: join.case,
    replacements: join.replacements,
    description: join.description,
  };
  switch (style) {
    case "possessive":
      return { ...shared, style, suffix: "s" };
    case "curated_variant":
      return {
        description: join.description,
        style,
        source_field: "approved_names",
        variant_from_result_id: 1,
        on_missing: "error",
      };
    default:
      return { ...shared, style };
  }
}

/** The naming methods a method table drives, each edited and saved alone. */
export function NamingMethodsEditor({
  detail,
  onSaved,
}: {
  detail: TableDetail;
  onSaved: (detail: TableDetail) => void;
}) {
  const methods = detail.naming_methods ?? [];
  if (methods.length === 0) return null;
  return (
    <section className="editor-section" aria-labelledby="naming-methods">
      <h3 id="naming-methods" className="label">
        Naming methods
      </h3>
      <p className="field-help">
        How each method joins its rolled parts, and the exact spellings used for particular
        combinations. Saved separately from the table above.
      </p>
      {methods.map((m) => (
        <NamingMethodCard key={`${m.scheme.id}/${m.method.id}`} saved={m} onSaved={onSaved} />
      ))}
    </section>
  );
}

function NamingMethodCard({
  saved,
  onSaved,
}: {
  saved: NamingMethodDetail;
  onSaved: (detail: TableDetail) => void;
}) {
  const [join, setJoin] = useState<JoinRule>(saved.join);
  const [exceptions, setExceptions] = useState<SpellingException[]>(saved.exceptions);
  const [open, setOpen] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setJoin(saved.join);
    setExceptions(saved.exceptions);
  }, [saved]);
  const dirty =
    JSON.stringify(join) !== JSON.stringify(saved.join) ||
    JSON.stringify(exceptions) !== JSON.stringify(saved.exceptions);
  const check = useDraftCheck<
    { join: JoinRule; exceptions: SpellingException[] } | null,
    DraftCheck
  >(dirty ? { join, exceptions } : null, (d) =>
    content.checkNamingMethod(saved.scheme.id, saved.method.id, d!.join, d!.exceptions),
  );
  const live = dirty ? check : null;
  const hasErrors =
    Boolean(live?.error) || (live?.issues ?? []).some((i) => i.severity === "error");
  const run = async (call: Promise<TableDetail>, done: string) => {
    setBusy(true);
    try {
      onSaved(await call);
      setMessage({ text: done, error: false });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };
  const name = saved.method.name;
  const curated = join.style === "curated_variant";
  return (
    <div className="naming-method">
      <button
        type="button"
        className="link-button naming-method-toggle"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {name}
      </button>{" "}
      <span className="muted small">
        {saved.scheme.name} · {join.style} · {exceptions.length} exception
        {exceptions.length === 1 ? "" : "s"}
        {saved.modified ? " · customised" : ""}
      </span>
      {open && (
        <div className="naming-method-body">
          <p className="field-help">
            Parts: {saved.components.map((c) => c.table.name).join(" + ")}
          </p>
          <div className="field-grid">
            <label className="field">
              <span className="field-label">Join style</span>
              <select
                className="select select-inline"
                aria-label={`${name}: join style`}
                value={join.style}
                onChange={(e) => setJoin(withStyle(join, e.target.value as JoinRule["style"]))}
              >
                {STYLES.map((s) => (
                  <option key={s.value} value={s.value}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
            {curated && join.style === "curated_variant" && (
              <>
                <PartSelect
                  label={`${name}: source part`}
                  title="Approved names are read from"
                  parts={saved.components.map((c) => c.table)}
                  value={join.source_from_result_id ?? 0}
                  onChange={(i) => setJoin({ ...join, source_from_result_id: i })}
                />
                <PartSelect
                  label={`${name}: variant part`}
                  title="The approved name is chosen by"
                  parts={saved.components.map((c) => c.table)}
                  value={join.variant_from_result_id}
                  onChange={(i) => setJoin({ ...join, variant_from_result_id: i })}
                />
              </>
            )}
            {!curated && (
              <>
                <label className="field">
                  <span className="field-label">Separator</span>
                  <input
                    className="text-input"
                    aria-label={`${name}: separator`}
                    value={join.separator ?? ""}
                    placeholder="None"
                    onChange={(e) => setJoin({ ...join, separator: e.target.value || undefined })}
                  />
                </label>
                <label className="field">
                  <span className="field-label">Capitalisation</span>
                  <select
                    className="select select-inline"
                    aria-label={`${name}: capitalisation`}
                    value={join.case ?? "sentence"}
                    onChange={(e) =>
                      setJoin({
                        ...join,
                        case:
                          e.target.value === "sentence"
                            ? undefined
                            : (e.target.value as Capitalisation),
                      })
                    }
                  >
                    {CASES.map((c) => (
                      <option key={c.value} value={c.value}>
                        {c.label}
                      </option>
                    ))}
                  </select>
                </label>
              </>
            )}
            {join.style === "possessive" && (
              <>
                <label className="field">
                  <span className="field-label">Possessive suffix</span>
                  <input
                    className="text-input"
                    aria-label={`${name}: possessive suffix`}
                    value={join.suffix}
                    onChange={(e) => setJoin({ ...join, suffix: e.target.value })}
                  />
                </label>
                <label className="field">
                  <span className="field-label">Only before endings (IDs)</span>
                  <input
                    className="text-input"
                    aria-label={`${name}: before endings`}
                    value={(join.before ?? []).join(", ")}
                    placeholder="Every ending"
                    onChange={(e) => {
                      const ids = e.target.value
                        .split(",")
                        .map((x) => x.trim())
                        .filter(Boolean);
                      setJoin({ ...join, before: ids.length ? ids : undefined });
                    }}
                  />
                </label>
                <label className="field">
                  <span className="field-label">After a final s</span>
                  <input
                    className="text-input"
                    aria-label={`${name}: after a final s`}
                    value={join.after_s ?? ""}
                    placeholder="None"
                    onChange={(e) => setJoin({ ...join, after_s: e.target.value || undefined })}
                  />
                </label>
              </>
            )}
          </div>
          {curated && (
            <p className="field-help">
              The source part&apos;s approved name for the variant rolled is used exactly as
              written. A missing name is an error, never a fallback.
            </p>
          )}
          <label className="field">
            <span className="field-label">Note</span>
            <input
              className="text-input"
              aria-label={`${name}: note`}
              value={join.description ?? ""}
              onChange={(e) => setJoin({ ...join, description: e.target.value || undefined })}
            />
          </label>

          <ExceptionsEditor
            name={name}
            components={saved.components}
            exceptions={exceptions}
            onChange={setExceptions}
          />

          {live && <IssueList issues={live.issues} error={live.error} />}
          <div className="editor-actions">
            <button
              type="button"
              className="btn btn-primary btn-small"
              disabled={!dirty || hasErrors || busy || !live}
              onClick={() =>
                run(
                  content.saveNamingMethod(saved.scheme.id, saved.method.id, join, exceptions),
                  "Naming method saved",
                )
              }
            >
              Save {name}
            </button>
            {dirty && (
              <button
                type="button"
                className="btn btn-small"
                onClick={() => {
                  setJoin(saved.join);
                  setExceptions(saved.exceptions);
                }}
              >
                Revert
              </button>
            )}
            {saved.built_in && saved.modified && (
              <button
                type="button"
                className="btn btn-small"
                disabled={busy}
                onClick={() =>
                  run(
                    content.resetNamingMethod(saved.scheme.id, saved.method.id),
                    "Naming method reset to default",
                  )
                }
              >
                Reset {name} to default
              </button>
            )}
            {message && (
              <span className={message.error ? "field-error" : "issues-ok"} role="status">
                {message.text}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function PartSelect({
  label,
  title,
  parts,
  value,
  onChange,
}: {
  label: string;
  title: string;
  parts: Named[];
  value: number;
  onChange: (index: number) => void;
}) {
  return (
    <label className="field">
      <span className="field-label">{title}</span>
      <select
        className="select select-inline"
        aria-label={label}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      >
        {parts.map((p, i) => (
          <option key={i} value={i}>
            Part {i + 1}: {p.name}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Spelling exceptions: one result per component, and the exact name. */
function ExceptionsEditor({
  name,
  components,
  exceptions,
  onChange,
}: {
  name: string;
  components: NamingMethodDetail["components"];
  exceptions: SpellingException[];
  onChange: (next: SpellingException[]) => void;
}) {
  // By-name exceptions are shown by the result they name, and stored by
  // result ID once edited, so renaming a result never breaks them.
  const idsOf = (e: SpellingException): string[] =>
    components.map((c, i) => {
      if (e.results?.length) return e.results[i] ?? "";
      const byName = e.components?.[i];
      return c.results.find((r) => r.name.trim() === byName?.trim())?.id ?? "";
    });
  const update = (k: number, patch: Partial<SpellingException>) =>
    onChange(exceptions.map((e, j) => (j === k ? { ...e, ...patch } : e)));
  return (
    <div className="exceptions">
      <span className="field-label">Spelling exceptions</span>
      <span className="field-help"> Checked before the join; the name is used as written.</span>
      {exceptions.length > 0 && (
        <div className="edit-grid-scroll">
          <table className="edit-grid">
            <thead>
              <tr>
                {components.map((c, i) => (
                  <th key={i} scope="col">
                    {c.table.name}
                  </th>
                ))}
                <th scope="col">Name</th>
                <th scope="col">Note</th>
                <th scope="col">
                  <span className="visually-hidden">Remove</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {exceptions.map((e, k) => {
                const ids = idsOf(e);
                return (
                  <tr key={k}>
                    {components.map((c, i) => (
                      <td key={i}>
                        <select
                          className="select select-inline"
                          aria-label={`${name}: exception ${k + 1} part ${i + 1}`}
                          value={ids[i]}
                          onChange={(ev) => {
                            const results = [...ids];
                            results[i] = ev.target.value;
                            update(k, { results, components: undefined });
                          }}
                        >
                          <option value="" disabled>
                            Choose…
                          </option>
                          {c.results.map((r) => (
                            <option key={r.id} value={r.id}>
                              {r.name}
                            </option>
                          ))}
                        </select>
                      </td>
                    ))}
                    <td>
                      <input
                        className="text-input"
                        aria-label={`${name}: exception ${k + 1} name`}
                        value={e.name}
                        onChange={(ev) => update(k, { name: ev.target.value })}
                      />
                    </td>
                    <td>
                      <input
                        className="text-input"
                        aria-label={`${name}: exception ${k + 1} note`}
                        value={e.note ?? ""}
                        onChange={(ev) => update(k, { note: ev.target.value || undefined })}
                      />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="icon-button"
                        aria-label={`Remove exception ${k + 1}`}
                        onClick={() => onChange(exceptions.filter((_, j) => j !== k))}
                      >
                        ×
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <button
        type="button"
        className="btn btn-small"
        onClick={() =>
          onChange([
            ...exceptions,
            { results: components.map((c) => c.results[0]?.id ?? ""), name: "" },
          ])
        }
      >
        Add exception
      </button>
    </div>
  );
}
