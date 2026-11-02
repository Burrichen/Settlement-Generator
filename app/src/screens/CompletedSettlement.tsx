import { useState } from "react";
import { generation, type SessionView } from "../api";
import { ConfirmBar, NameForm } from "../components/generation";
import { SettlementSheet, foldSubStages } from "../components/SettlementSheet";
import { SettlementTabs, type SettlementTab } from "../components/SettlementTabs";

/**
 * The finished settlement. Shows each stage's result; tags only with Tag
 * View. Actions: Re-roll (whole settlement), Save (named snapshot), Delete
 * (discard).
 */
export function CompletedSettlement({
  view,
  busy,
  error,
  onReroll,
  onSaved,
  onDiscarded,
}: {
  view: SessionView;
  busy: boolean;
  error: string | null;
  onReroll: () => void;
  onSaved: () => void;
  onDiscarded: () => void;
}) {
  const [naming, setNaming] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [tab, setTab] = useState<SettlementTab>("details");

  const save = async (name: string) => {
    setSaving(true);
    setSaveError(null);
    try {
      await generation.save(name);
      onSaved();
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const discard = async () => {
    await generation.discard().catch(() => undefined);
    onDiscarded();
  };

  return (
    <div className="completed">
      <SettlementTabs active={tab} onChange={setTab}>
        {tab === "map" ? (
          <div className="map-unsaved">
            <p className="display map-empty-title">No map yet</p>
            <p className="muted">
              Maps belong to saved settlements. Save this settlement, then open its Map tab in Saved
              Settlements to draw one. A map is never drawn automatically.
            </p>
          </div>
        ) : (
          <SettlementSheet
            label="Generated settlement"
            rows={foldSubStages(
              view.stages.map((stage) => ({
                key: String(stage.index),
                label: stage.stage.name,
                value: stage.result?.name ?? (stage.skipped ? "Not applicable" : "—"),
                source: stage.table.name !== stage.stage.name ? stage.table.name : undefined,
                tags: stage.tags,
                subStage: stage.sub_stage,
                // Roles are shown with each Authority Figure instead.
                skipped: stage.status === "skipped" || !!stage.roles,
                figures: stage.figures?.record,
                locations: stage.locations?.record,
              })),
            )}
            meta="Not saved yet"
          />
        )}
      </SettlementTabs>

      {error && (
        <p className="alert alert-inline" role="alert">
          {error}
        </p>
      )}

      {naming ? (
        <NameForm
          busy={saving}
          serverError={saveError}
          initialName={view.suggested_name}
          onSave={save}
          onCancel={() => setNaming(false)}
        />
      ) : confirmDelete ? (
        <ConfirmBar
          message="Delete this settlement? It has not been saved and cannot be recovered."
          confirmLabel="Delete"
          onConfirm={discard}
          onCancel={() => setConfirmDelete(false)}
        />
      ) : (
        <div className="action-bar">
          <button type="button" className="btn" disabled={busy} onClick={onReroll}>
            Re-roll
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy}
            onClick={() => setNaming(true)}
          >
            Save
          </button>
          <button
            type="button"
            className="btn btn-quiet-danger"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
          >
            Delete
          </button>
        </div>
      )}
    </div>
  );
}
