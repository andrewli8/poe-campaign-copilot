import { useState } from "react";
import "./SettingsPage.css";
import type { AppConfig, PobSummary, RouteVariant } from "./types";

export interface SettingsPageProps {
  config: AppConfig;
  onPick: () => void;
  onImportPreview: (code: string) => void;
  preview: PobSummary | null;
  previewError: string | null;
  onSave: (cfg: AppConfig) => void;
  saving: boolean;
  savedAt: number | null;
}

const VARIANTS: { value: RouteVariant; label: string }[] = [
  { value: "league-start", label: "League start (fresh, recommended default)" },
  { value: "standard", label: "Standard (existing character)" },
];

/// Presentational settings form. The log path is always displayed straight
/// from `config` (picking a new one is delegated to `onPick`, which the
/// container resolves by re-invoking `pick_log_file` and folding the result
/// back into `config`); the route variant and PoB text are edited locally
/// here and only reported upward — as a single `AppConfig` — when the user
/// clicks Save, so a half-finished edit can never leak into `apply_settings`
/// a keystroke at a time.
export function SettingsPage({
  config,
  onPick,
  onImportPreview,
  preview,
  previewError,
  onSave,
  saving,
  savedAt,
}: SettingsPageProps) {
  const [variant, setVariant] = useState<RouteVariant>(config.variant);
  const [pobText, setPobText] = useState(config.pob_code ?? "");

  function handleSave() {
    const trimmed = pobText.trim();
    onSave({
      client_log_path: config.client_log_path,
      variant,
      pob_code: trimmed === "" ? null : trimmed,
    });
  }

  return (
    <div className="settings-page">
      <h1 className="settings-title">Settings</h1>

      <section className="settings-row">
        <label className="settings-label">Client.txt log path</label>
        <div className="log-path-row">
          <span className={["log-path", !config.client_log_path && "unset"].filter(Boolean).join(" ")}>
            {config.client_log_path ?? "Not set"}
          </span>
          <button type="button" className="btn btn-secondary" onClick={onPick}>
            Browse&hellip;
          </button>
        </div>
      </section>

      <section className="settings-row">
        <label className="settings-label" htmlFor="variant-select">
          Route variant
        </label>
        <select
          id="variant-select"
          className="settings-select"
          value={variant}
          onChange={(e) => setVariant(e.target.value as RouteVariant)}
        >
          {VARIANTS.map((v) => (
            <option key={v.value} value={v.value}>
              {v.label}
            </option>
          ))}
        </select>
      </section>

      <section className="settings-row">
        <label className="settings-label" htmlFor="pob-textarea">
          Path of Building import (optional)
        </label>
        <textarea
          id="pob-textarea"
          className="settings-textarea"
          placeholder="Paste a PoB share code or XML export for build reminders"
          value={pobText}
          onChange={(e) => setPobText(e.target.value)}
          rows={4}
        />
        <div className="preview-row">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => onImportPreview(pobText)}
          >
            Preview import
          </button>
        </div>

        {preview && (
          <div className="preview-card">
            <div className="preview-heading">
              {preview.class_name}
              {preview.ascend_name ? ` — ${preview.ascend_name}` : ""}
            </div>
            <div className="preview-detail">{preview.milestone_count} milestones</div>
            <span className={`reliability-badge reliability-${preview.reliability}`}>
              {preview.reliability}
            </span>
          </div>
        )}

        {previewError && <div className="preview-error">{previewError}</div>}
      </section>

      <section className="settings-actions">
        <button
          type="button"
          className="btn btn-primary"
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? "Saving…" : "Save"}
        </button>
        {!saving && savedAt !== null && <span className="saved-confirmation">Saved &#10003;</span>}
      </section>
    </div>
  );
}
