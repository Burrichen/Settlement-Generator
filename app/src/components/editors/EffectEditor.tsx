// Effects are edited from the core's effect schema: every kind and field is
// described by data, so new effect kinds need no new interface code.
import type { EffectDef, EffectField, EffectKindSchema } from "../../api";
import { TableSelect, TagSelect, useTableDetail } from "./common";

/** A blank effect of `kind`, with only the discriminator set. */
export function blankEffect(schema: EffectKindSchema): EffectDef {
  return { kind: schema.kind, ...(schema.executable ? {} : { status: "planned" }) };
}

export function EffectEditor({
  effect,
  schema,
  label,
  onChange,
  onRemove,
}: {
  effect: EffectDef;
  schema: EffectKindSchema[];
  label: string;
  onChange: (effect: EffectDef) => void;
  onRemove: () => void;
}) {
  const kind = schema.find((s) => s.kind === effect.kind);
  const status = effect.status ?? "active";
  const set = (name: string, value: unknown) => {
    const next: EffectDef = { ...effect };
    if (value === undefined || value === "" || (Array.isArray(value) && value.length === 0)) {
      delete next[name];
    } else {
      next[name] = value;
    }
    onChange(next);
  };

  return (
    <fieldset className="effect">
      <legend className="visually-hidden">{label}</legend>
      <div className="effect-head">
        <select
          className="select select-inline"
          aria-label={`${label}: kind`}
          value={effect.kind}
          onChange={(e) => {
            const next = schema.find((s) => s.kind === e.target.value);
            if (next) onChange({ ...blankEffect(next), note: effect.note });
          }}
        >
          {schema.map((s) => (
            <option key={s.kind} value={s.kind}>
              {s.label}
            </option>
          ))}
        </select>
        <select
          className="select select-inline"
          aria-label={`${label}: status`}
          value={status}
          disabled={kind && !kind.executable}
          onChange={(e) => set("status", e.target.value === "active" ? undefined : "planned")}
        >
          <option value="active">Active</option>
          <option value="planned">Planned (not executed)</option>
        </select>
        <button
          type="button"
          className="icon-button"
          aria-label={`Remove ${label}`}
          onClick={onRemove}
        >
          ×
        </button>
      </div>
      {kind && <p className="muted small effect-description">{kind.description}</p>}
      <div className="effect-fields">
        {kind?.fields.map((field) => (
          <label key={field.name} className="field">
            <span className="field-label">
              {field.label}
              {field.required && (
                <span className="required" aria-label="required">
                  {" "}
                  *
                </span>
              )}
            </span>
            <FieldInput
              field={field}
              effect={effect}
              label={`${label}: ${field.label}`}
              onChange={(v) => set(field.name, v)}
            />
            {field.help && <span className="field-help">{field.help}</span>}
          </label>
        ))}
        <label className="field field-wide">
          <span className="field-label">Note</span>
          <input
            className="text-input"
            aria-label={`${label}: note`}
            value={(effect.note as string | undefined) ?? ""}
            onChange={(e) => set("note", e.target.value)}
          />
        </label>
      </div>
    </fieldset>
  );
}

function FieldInput({
  field,
  effect,
  label,
  onChange,
}: {
  field: EffectField;
  effect: EffectDef;
  label: string;
  onChange: (value: unknown) => void;
}) {
  const value = effect[field.name];
  switch (field.kind.type) {
    case "table":
      return <TableSelect label={label} value={(value as string) ?? ""} onChange={onChange} />;
    case "tag":
      return <TagSelect label={label} value={(value as string) ?? ""} onChange={onChange} />;
    case "variant":
    case "result":
    case "result_list":
      return (
        <TableItemPicker
          tableId={effect[field.kind.table_field] as string | undefined}
          kind={field.kind.type}
          value={value}
          label={label}
          onChange={onChange}
        />
      );
    case "integer":
      return (
        <input
          type="number"
          className="num-input"
          aria-label={label}
          value={(value as number | undefined) ?? ""}
          onChange={(e) =>
            onChange(e.target.value === "" ? undefined : Number.parseInt(e.target.value, 10))
          }
        />
      );
    case "choice":
      return (
        <select
          className="select select-inline"
          aria-label={label}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Choose…</option>
          {field.kind.options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      );
    default:
      return (
        <input
          className="text-input"
          aria-label={label}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}

/** Picks variants or results of the table named by another field. */
function TableItemPicker({
  tableId,
  kind,
  value,
  label,
  onChange,
}: {
  tableId: string | undefined;
  kind: "variant" | "result" | "result_list";
  value: unknown;
  label: string;
  onChange: (value: unknown) => void;
}) {
  const detail = useTableDetail(tableId);
  if (!tableId) return <span className="muted small">Choose a table first.</span>;
  const items =
    kind === "variant"
      ? (detail?.table.variants ?? []).map((v) => ({ id: v.id, name: v.name }))
      : (detail?.table.results ?? []).map((r) => ({ id: r.id, name: r.name }));

  if (kind === "result_list") {
    const selected = (value as string[] | undefined) ?? [];
    return (
      <span className="check-list" role="group" aria-label={label}>
        {items.map((item) => (
          <label key={item.id} className="check">
            <input
              type="checkbox"
              checked={selected.includes(item.id)}
              onChange={(e) =>
                onChange(
                  e.target.checked ? [...selected, item.id] : selected.filter((s) => s !== item.id),
                )
              }
            />
            {item.name}
          </label>
        ))}
        {items.length === 0 && <span className="muted small">No results to choose.</span>}
      </span>
    );
  }
  return (
    <select
      className="select select-inline"
      aria-label={label}
      value={(value as string) ?? ""}
      onChange={(e) => onChange(e.target.value || undefined)}
    >
      <option value="">{kind === "variant" ? "None" : "Choose…"}</option>
      {items.map((item) => (
        <option key={item.id} value={item.id}>
          {item.name}
        </option>
      ))}
    </select>
  );
}
