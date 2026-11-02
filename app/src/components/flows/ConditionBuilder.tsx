// A plain-language condition builder: WHEN [subject] [is] [value] AND/OR …
// It edits the core's condition data directly; groups give ALL (AND), ANY
// (OR) and NONE of their tests, and can nest.
import type { Condition, FlowContext } from "../../api";

type Test =
  | { type: "result"; step: string; negate: boolean; results: string[] }
  | { type: "value"; step: string; min?: number; max?: number }
  | { type: "tag"; tag: string; negate: boolean }
  | { type: "setting"; setting: string; negate: boolean; value?: string }
  | { type: "setting_num"; setting: string; min?: number; max?: number }
  | { type: "setting_has"; setting: string; negate: boolean; value: string }
  | { type: "group"; group: Group }
  | { type: "raw"; condition: Condition };

type Mode = "all" | "any" | "none";

interface Group {
  mode: Mode;
  tests: Test[];
}

// ----- condition ⇄ group ------------------------------------------------------------

function parseTest(c: Condition): Test {
  if ("has_tag" in c) return { type: "tag", tag: c.has_tag, negate: false };
  if ("setting_on" in c) return { type: "setting", setting: c.setting_on, negate: false };
  if ("setting_is" in c)
    return {
      type: "setting",
      setting: c.setting_is.setting,
      negate: false,
      value: c.setting_is.value,
    };
  if ("setting_between" in c)
    return {
      type: "setting_num",
      setting: c.setting_between.setting,
      min: c.setting_between.min,
      max: c.setting_between.max,
    };
  if ("setting_has" in c)
    return {
      type: "setting_has",
      setting: c.setting_has.setting,
      negate: false,
      value: c.setting_has.value,
    };
  if ("result_is" in c)
    return {
      type: "result",
      step: c.result_is.step,
      negate: false,
      results: c.result_is.results,
    };
  if ("value_in" in c)
    return { type: "value", step: c.value_in.step, min: c.value_in.min, max: c.value_in.max };
  if ("not" in c) {
    const inner = parseTest(c.not);
    if (
      inner.type === "tag" ||
      inner.type === "setting" ||
      inner.type === "setting_has" ||
      inner.type === "result"
    )
      return { ...inner, negate: !inner.negate };
    if (inner.type === "group" && inner.group.mode === "any")
      return { type: "group", group: { ...inner.group, mode: "none" } };
    return { type: "raw", condition: c };
  }
  return { type: "group", group: parseGroup(c) };
}

function parseGroup(c: Condition | undefined): Group {
  if (!c) return { mode: "all", tests: [] };
  if ("all" in c) return { mode: "all", tests: c.all.map(parseTest) };
  if ("any" in c) return { mode: "any", tests: c.any.map(parseTest) };
  if ("not" in c && "any" in c.not) return { mode: "none", tests: c.not.any.map(parseTest) };
  return { mode: "all", tests: [parseTest(c)] };
}

function testCondition(t: Test): Condition {
  const negated = (c: Condition, negate: boolean): Condition => (negate ? { not: c } : c);
  switch (t.type) {
    case "tag":
      return negated({ has_tag: t.tag }, t.negate);
    case "setting":
      return negated(
        t.value === undefined
          ? { setting_on: t.setting }
          : { setting_is: { setting: t.setting, value: t.value } },
        t.negate,
      );
    case "setting_num": {
      const setting_between: { setting: string; min?: number; max?: number } = {
        setting: t.setting,
      };
      if (t.min !== undefined) setting_between.min = t.min;
      if (t.max !== undefined) setting_between.max = t.max;
      return { setting_between };
    }
    case "setting_has":
      return negated({ setting_has: { setting: t.setting, value: t.value } }, t.negate);
    case "result":
      return negated({ result_is: { step: t.step, results: t.results } }, t.negate);
    case "value": {
      const value_in: { step: string; min?: number; max?: number } = { step: t.step };
      if (t.min !== undefined) value_in.min = t.min;
      if (t.max !== undefined) value_in.max = t.max;
      return { value_in };
    }
    case "group":
      return groupCondition(t.group);
    case "raw":
      return t.condition;
  }
}

function groupCondition(g: Group): Condition {
  const items = g.tests.map(testCondition);
  if (g.mode === "none") return { not: { any: items } };
  if (items.length === 1) return items[0];
  return g.mode === "all" ? { all: items } : { any: items };
}

// ----- the builder ------------------------------------------------------------------

/** A new test about `subject` (`step:…`, `tag:…` or `setting:…`). */
function testFor(subject: string, context: FlowContext): Test {
  const [type, ...rest] = subject.split(":");
  const id = rest.join(":");
  if (type === "tag") return { type: "tag", tag: id, negate: false };
  if (type === "setting") {
    const setting = context.settings.find((s) => s.id === id);
    switch (setting?.kind) {
      case "number":
        return { type: "setting_num", setting: id, min: 1 };
      case "multi_select":
        return {
          type: "setting_has",
          setting: id,
          negate: false,
          value: setting.options[0]?.id ?? "",
        };
      case "text":
        return { type: "setting", setting: id, negate: false, value: "" };
      case "choice":
        return { type: "setting", setting: id, negate: false, value: setting.options[0]?.id ?? "" };
      default:
        return { type: "setting", setting: id, negate: false };
    }
  }
  const step = context.subjects.find((s) => s.step === id);
  if (step && step.results.length === 0 && step.numeric) return { type: "value", step: id, min: 1 };
  return {
    type: "result",
    step: id,
    negate: false,
    results: step?.results[0] ? [step.results[0].id] : [],
  };
}

function subjectOf(t: Test): string {
  switch (t.type) {
    case "result":
    case "value":
      return `step:${t.step}`;
    case "tag":
      return `tag:${t.tag}`;
    case "setting":
    case "setting_num":
    case "setting_has":
      return `setting:${t.setting}`;
    default:
      return "";
  }
}

const MODE_WORD: Record<Mode, string> = { all: "AND", any: "OR", none: "OR" };

export function ConditionBuilder({
  value,
  onChange,
  context,
  label,
}: {
  value: Condition | undefined;
  onChange: (next: Condition) => void;
  context: FlowContext;
  /** Names the builder for assistive technology, e.g. "Case 1 condition". */
  label: string;
}) {
  const group = parseGroup(value);
  return (
    <div className="cond-builder" role="group" aria-label={label}>
      <GroupEditor
        group={group}
        onChange={(g) => onChange(g.tests.length === 0 ? { all: [] } : groupCondition(g))}
        context={context}
        path={label}
        top
      />
    </div>
  );
}

function GroupEditor({
  group,
  onChange,
  context,
  path,
  top = false,
}: {
  group: Group;
  onChange: (g: Group) => void;
  context: FlowContext;
  path: string;
  top?: boolean;
}) {
  const setTest = (i: number, t: Test) =>
    onChange({ ...group, tests: group.tests.map((x, j) => (j === i ? t : x)) });
  const remove = (i: number) =>
    onChange({ ...group, tests: group.tests.filter((_, j) => j !== i) });
  const firstSubject =
    (context.subjects[0] && `step:${context.subjects[0].step}`) ||
    (context.tags[0] && `tag:${context.tags[0].id}`) ||
    (context.settings[0] && `setting:${context.settings[0].id}`);
  return (
    <div className={`cond-group ${top ? "is-top" : ""}`}>
      {group.tests.length > 1 || !top || group.mode !== "all" ? (
        <label className="cond-mode">
          <span className="sr-only">{path} joins its tests with</span>
          <select
            className="select select-inline"
            aria-label={`${path} match`}
            value={group.mode}
            onChange={(e) => onChange({ ...group, mode: e.target.value as Mode })}
          >
            <option value="all">All of these (AND)</option>
            <option value="any">Any of these (OR)</option>
            <option value="none">None of these</option>
          </select>
        </label>
      ) : null}
      {group.tests.length === 0 && (
        <p className="cond-always muted">{top ? "Always applies." : "Empty group."}</p>
      )}
      <ol className="cond-tests">
        {group.tests.map((test, i) => (
          <li key={i} className="cond-test">
            <span className="cond-word">
              {i === 0 ? (top ? "WHEN" : "") : MODE_WORD[group.mode]}
            </span>
            {test.type === "group" ? (
              <div className="cond-nested">
                <GroupEditor
                  group={test.group}
                  onChange={(g) => setTest(i, { type: "group", group: g })}
                  context={context}
                  path={`${path} group ${i + 1}`}
                />
              </div>
            ) : test.type === "raw" ? (
              <code className="cond-raw">{JSON.stringify(test.condition)}</code>
            ) : (
              <TestEditor
                test={test}
                onChange={(t) => setTest(i, t)}
                context={context}
                label={`${path} test ${i + 1}`}
              />
            )}
            <button
              type="button"
              className="btn btn-small btn-quiet-danger"
              aria-label={`Remove ${path} test ${i + 1}`}
              onClick={() => remove(i)}
            >
              ×
            </button>
          </li>
        ))}
      </ol>
      <div className="button-row">
        <button
          type="button"
          className="btn btn-small"
          disabled={!firstSubject}
          onClick={() =>
            onChange({ ...group, tests: [...group.tests, testFor(firstSubject!, context)] })
          }
        >
          {group.tests.length === 0 ? "Add a test" : `Add ${MODE_WORD[group.mode]} test`}
        </button>
        <button
          type="button"
          className="btn btn-small"
          disabled={!firstSubject}
          onClick={() =>
            onChange({
              ...group,
              tests: [
                ...group.tests,
                {
                  type: "group",
                  group: {
                    mode: group.mode === "all" ? "any" : "all",
                    tests: [testFor(firstSubject!, context)],
                  },
                },
              ],
            })
          }
        >
          Add group
        </button>
      </div>
    </div>
  );
}

function TestEditor({
  test,
  onChange,
  context,
  label,
}: {
  test: Exclude<Test, { type: "group" } | { type: "raw" }>;
  onChange: (t: Test) => void;
  context: FlowContext;
  label: string;
}) {
  const subject = subjectOf(test);
  const step =
    test.type === "result" || test.type === "value"
      ? context.subjects.find((s) => s.step === test.step)
      : undefined;
  const setting =
    test.type === "setting" || test.type === "setting_num" || test.type === "setting_has"
      ? context.settings.find((s) => s.id === test.setting)
      : undefined;
  const known = (prefix: string, ids: string[], id: string) =>
    ids.includes(id) || !subject.startsWith(prefix);

  const operator = (() => {
    switch (test.type) {
      case "result":
        return test.negate ? "is_not" : "is";
      case "value":
      case "setting_num":
        return test.min !== undefined && test.max !== undefined
          ? test.min === test.max
            ? "exactly"
            : "between"
          : test.max !== undefined
            ? "at_most"
            : "at_least";
      case "tag":
        return test.negate ? "absent" : "present";
      case "setting_has":
        return test.negate ? "excludes" : "includes";
      case "setting":
        return test.negate
          ? test.value === undefined
            ? "off"
            : "is_not"
          : test.value === undefined
            ? "on"
            : "is";
    }
  })();

  const setOperator = (op: string) => {
    if (test.type === "tag") return onChange({ ...test, negate: op === "absent" });
    if (test.type === "setting")
      return onChange({ ...test, negate: op === "off" || op === "is_not" });
    if (test.type === "setting_has") return onChange({ ...test, negate: op === "excludes" });
    if (test.type === "setting_num") {
      const lo = test.min ?? test.max ?? 1;
      const hi = test.max ?? test.min ?? lo;
      const range =
        op === "between"
          ? { min: lo, max: Math.max(hi, lo + 1) }
          : op === "exactly"
            ? { min: lo, max: lo }
            : op === "at_least"
              ? { min: lo }
              : { max: hi };
      return onChange({ type: "setting_num", setting: test.setting, ...range });
    }
    if (step && ["between", "at_least", "at_most", "exactly"].includes(op)) {
      const current = test.type === "value" ? test : { min: undefined, max: undefined };
      const lo = current.min ?? current.max ?? 1;
      const hi = current.max ?? current.min ?? lo;
      const range =
        op === "between"
          ? { min: lo, max: Math.max(hi, lo + 1) }
          : op === "exactly"
            ? { min: lo, max: lo }
            : op === "at_least"
              ? { min: lo }
              : { max: hi };
      return onChange({ type: "value", step: step.step, ...range });
    }
    if (op === "is" || op === "is_not") {
      const results =
        test.type === "result" ? test.results : step?.results[0] ? [step.results[0].id] : [];
      return onChange({ type: "result", step: step?.step ?? "", negate: op === "is_not", results });
    }
  };

  const num = (v: string): number | undefined => (v.trim() === "" ? undefined : Number(v));

  return (
    <span className="cond-row">
      <select
        className="select select-inline"
        aria-label={`${label} subject`}
        value={subject}
        onChange={(e) => onChange(testFor(e.target.value, context))}
      >
        {context.subjects.length > 0 && (
          <optgroup label="Earlier results">
            {context.subjects.map((s) => (
              <option key={s.step} value={`step:${s.step}`}>
                {s.label}
              </option>
            ))}
          </optgroup>
        )}
        {!known(
          "step:",
          context.subjects.map((s) => `step:${s.step}`),
          subject,
        ) && <option value={subject}>{subject.slice(5)} (missing)</option>}
        <optgroup label="Tags">
          {context.tags.map((t) => (
            <option key={t.id} value={`tag:${t.id}`}>
              {t.name}
            </option>
          ))}
        </optgroup>
        <optgroup label="Settings">
          {context.settings.map((s) => (
            <option key={s.id} value={`setting:${s.id}`}>
              {s.name}
            </option>
          ))}
        </optgroup>
      </select>
      <select
        className="select select-inline"
        aria-label={`${label} operator`}
        value={operator}
        onChange={(e) => setOperator(e.target.value)}
      >
        {test.type === "tag" && (
          <>
            <option value="present">is present</option>
            <option value="absent">is not present</option>
          </>
        )}
        {test.type === "setting_has" && (
          <>
            <option value="includes">includes</option>
            <option value="excludes">does not include</option>
          </>
        )}
        {test.type === "setting_num" && (
          <>
            <option value="exactly">is exactly</option>
            <option value="between">is between</option>
            <option value="at_least">is at least</option>
            <option value="at_most">is at most</option>
          </>
        )}
        {test.type === "setting" &&
          (setting?.kind === "choice" || setting?.kind === "text" ? (
            <>
              <option value="is">is</option>
              <option value="is_not">is not</option>
            </>
          ) : (
            <>
              <option value="on">is on</option>
              <option value="off">is off</option>
            </>
          ))}
        {(test.type === "result" || test.type === "value") && (
          <>
            {(step?.results.length ?? 0) > 0 || test.type === "result" ? (
              <>
                <option value="is">is</option>
                <option value="is_not">is not</option>
              </>
            ) : null}
            {(step?.numeric || test.type === "value") && (
              <>
                <option value="exactly">is exactly</option>
                <option value="between">is between</option>
                <option value="at_least">is at least</option>
                <option value="at_most">is at most</option>
              </>
            )}
          </>
        )}
      </select>
      {test.type === "result" &&
        (test.results.length === 0 ? [""] : test.results).map((r, i) => (
          <span key={i} className="cond-value">
            {i > 0 && <span className="cond-or">or</span>}
            <select
              className="select select-inline"
              aria-label={`${label} value${i > 0 ? ` ${i + 1}` : ""}`}
              value={r}
              onChange={(e) =>
                onChange({
                  ...test,
                  results: (test.results.length === 0 ? [""] : test.results).map((x, j) =>
                    j === i ? e.target.value : x,
                  ),
                })
              }
            >
              {r === "" && <option value="">Choose…</option>}
              {(step?.results ?? []).map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
              {r !== "" && !step?.results.some((o) => o.id === r) && (
                <option value={r}>{r} (missing)</option>
              )}
            </select>
            {test.results.length > 1 && (
              <button
                type="button"
                className="btn btn-small"
                aria-label={`Remove ${label} value ${i + 1}`}
                onClick={() =>
                  onChange({ ...test, results: test.results.filter((_, j) => j !== i) })
                }
              >
                −
              </button>
            )}
          </span>
        ))}
      {test.type === "result" && (step?.results.length ?? 0) > test.results.length && (
        <button
          type="button"
          className="btn btn-small"
          aria-label={`${label}: add another result`}
          onClick={() => {
            const next = step!.results.find((o) => !test.results.includes(o.id));
            if (next) onChange({ ...test, results: [...test.results, next.id] });
          }}
        >
          + or
        </button>
      )}
      {(test.type === "value" || test.type === "setting_num") && (
        <>
          {test.min !== undefined && (
            <input
              className="num-input"
              type="number"
              aria-label={`${label} ${operator === "between" ? "lowest" : "number"}`}
              value={test.min}
              onChange={(e) =>
                onChange(
                  operator === "exactly"
                    ? { ...test, min: num(e.target.value), max: num(e.target.value) }
                    : { ...test, min: num(e.target.value) },
                )
              }
            />
          )}
          {operator === "between" && <span className="cond-or">and</span>}
          {test.max !== undefined && operator !== "exactly" && (
            <input
              className="num-input"
              type="number"
              aria-label={`${label} ${operator === "between" ? "highest" : "number"}`}
              value={test.max}
              onChange={(e) => onChange({ ...test, max: num(e.target.value) })}
            />
          )}
        </>
      )}
      {test.type === "setting" && setting?.kind === "text" && (
        <input
          className="text-input"
          aria-label={`${label} value`}
          value={test.value ?? ""}
          onChange={(e) => onChange({ ...test, value: e.target.value })}
        />
      )}
      {(test.type === "setting" || test.type === "setting_has") &&
        setting &&
        setting.kind !== "text" &&
        setting.options.length > 0 && (
          <select
            className="select select-inline"
            aria-label={`${label} value`}
            value={test.value ?? ""}
            onChange={(e) => onChange({ ...test, value: e.target.value })}
          >
            {setting.options.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        )}
    </span>
  );
}

/** A step's optional "only when" condition. */
export function OptionalWhen({
  value,
  onChange,
  context,
  label,
}: {
  value: Condition | undefined;
  onChange: (next: Condition | undefined) => void;
  context: FlowContext;
  label: string;
}) {
  if (!value) {
    return (
      <button type="button" className="btn btn-small" onClick={() => onChange({ all: [] })}>
        Only run when…
      </button>
    );
  }
  return (
    <div>
      <ConditionBuilder value={value} onChange={onChange} context={context} label={label} />
      <button type="button" className="btn btn-small" onClick={() => onChange(undefined)}>
        Always run
      </button>
    </div>
  );
}
