import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import type { ExportFormat, ExportResult, MapExportRequest } from "./types";

const FORMATS: { format: ExportFormat; label: string; help: string }[] = [
  { format: "png", label: "PNG picture", help: "For sharing and printing" },
  { format: "svg", label: "SVG drawing", help: "Sharp at any size; editable" },
  { format: "json", label: "Map data (JSON)", help: "Geometry, buildings and locations" },
];

/**
 * Exports the map on screen, or (for a City or Capital) every drawn
 * district map, each to its own file. Districts are never joined into one
 * picture. The core writes the files to the export folder.
 */
export function ExportDialog({
  mapId,
  mapLabel,
  districts,
  drawn,
  onExport,
  onClose,
}: {
  mapId: string;
  mapLabel: string;
  /** The settlement is mapped by district. */
  districts: boolean;
  /** How many maps are drawn (for "all districts"). */
  drawn: number;
  onExport: (request: MapExportRequest) => Promise<ExportResult>;
  onClose: () => void;
}) {
  const titleId = useId();
  const [formats, setFormats] = useState<ExportFormat[]>(["png"]);
  const [all, setAll] = useState(false);
  const [markers, setMarkers] = useState(true);
  const [legend, setLegend] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ExportResult | null>(null);
  const first = useRef<HTMLInputElement>(null);
  const done = useRef<HTMLButtonElement>(null);
  useEffect(() => first.current?.focus(), []);
  // The form is replaced by the result: keep focus inside the dialog.
  useEffect(() => {
    if (result) done.current?.focus();
  }, [result]);

  const toggle = (f: ExportFormat) =>
    setFormats((fs) => (fs.includes(f) ? fs.filter((x) => x !== f) : [...fs, f]));

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(
        await onExport({
          map: all ? undefined : mapId,
          formats,
          options: { markers, legend, width_px: 1600 },
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  };

  return (
    <div className="map-dialog-backdrop" onKeyDown={onKeyDown}>
      <div className="map-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <h3 id={titleId} className="map-dialog-title">
          Export {all ? "all district maps" : mapLabel}
        </h3>
        {result ? (
          <>
            <p>
              {result.files.length} file{result.files.length === 1 ? "" : "s"} saved in{" "}
              <code className="map-export-path">{result.dir}</code>:
            </p>
            <ul className="map-export-files">
              {result.files.map((f) => (
                <li key={f}>{f.split(/[\\/]/).pop()}</li>
              ))}
            </ul>
            <div className="action-bar map-dialog-actions">
              <button ref={done} type="button" className="btn btn-primary" onClick={onClose}>
                Done
              </button>
            </div>
          </>
        ) : (
          <>
            {districts && (
              <fieldset className="map-export-group">
                <legend className="field-label">Which maps</legend>
                <label className="check">
                  <input type="radio" checked={!all} onChange={() => setAll(false)} />
                  {mapLabel} only
                </label>
                <label className="check">
                  <input type="radio" checked={all} onChange={() => setAll(true)} />
                  Every drawn district ({drawn}), one file each
                </label>
              </fieldset>
            )}
            <fieldset className="map-export-group">
              <legend className="field-label">Formats</legend>
              {FORMATS.map(({ format, label, help }, i) => (
                <label key={format} className="check">
                  <input
                    ref={i === 0 ? first : undefined}
                    type="checkbox"
                    checked={formats.includes(format)}
                    onChange={() => toggle(format)}
                  />
                  {label} <span className="muted small">{help}</span>
                </label>
              ))}
            </fieldset>
            <fieldset className="map-export-group">
              <legend className="field-label">In pictures</legend>
              <label className="check">
                <input type="checkbox" checked={markers} onChange={() => setMarkers((m) => !m)} />
                Numbered markers
              </label>
              <label className="check">
                <input type="checkbox" checked={legend} onChange={() => setLegend((l) => !l)} />
                Legend
              </label>
            </fieldset>
            {error && (
              <p className="field-error" role="alert">
                {error}
              </p>
            )}
            <div className="action-bar map-dialog-actions">
              <button type="button" className="btn" onClick={onClose}>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy || formats.length === 0}
                onClick={() => void run()}
              >
                {busy ? "Exporting…" : "Export"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
