import type { ReactNode } from "react";
import { useState } from "react";
import "./SettingsPage.css";
import {
  HOTKEY_ACTIONS,
  normalizeHotkeyConfig,
  validateHotkeyConfig,
} from "./hotkeys";
import { OPACITY_MAX_PCT, OPACITY_MIN_PCT } from "./opacity";
import type { AppConfig, HotkeyConfig, PobSummary, RouteVariant } from "./types";

const OPACITY_SLIDER_STEP_PCT = 5;

export interface SettingsPageProps {
  config: AppConfig;
  onPick: () => void;
  onImportPreview: (code: string) => void;
  preview: PobSummary | null;
  previewError: string | null;
  onSave: (cfg: AppConfig) => void;
  saving: boolean;
  savedAt: number | null;
  // Live opacity preview: fired on every slider move with the fractional
  // opacity (0.2–1.0) so the container can push it to the overlay window
  // immediately, without waiting for Save. Optional so presentational
  // usages/tests that don't care about live preview stay minimal.
  onOpacityPreview?: (opacity: number) => void;
  // Optional slot rendered above the title, inside the styled settings
  // container — used by SettingsContainer to place <UpdateBanner/> so it
  // picks up the dark/gold settings background rather than sitting outside
  // it. Left undefined in existing callers/tests, which render nothing here.
  banner?: ReactNode;
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
  onOpacityPreview,
  banner,
}: SettingsPageProps) {
  const [variant, setVariant] = useState<RouteVariant>(config.variant);
  const [pobText, setPobText] = useState(config.pob_code ?? "");
  const [opacityPct, setOpacityPct] = useState(
    Math.round(config.overlay_opacity * 100),
  );
  const [hotkeys, setHotkeys] = useState<HotkeyConfig>(config.hotkeys);

  const hotkeyErrors = validateHotkeyConfig(hotkeys);
  const hasHotkeyErrors = Object.keys(hotkeyErrors).length > 0;

  function handleOpacityChange(pct: number) {
    setOpacityPct(pct);
    onOpacityPreview?.(pct / 100);
  }

  function handleHotkeyChange(action: keyof HotkeyConfig, value: string) {
    setHotkeys((prev) => ({ ...prev, [action]: value }));
  }

  function handleSave() {
    if (hasHotkeyErrors) {
      return; // Save is disabled too; belt-and-braces against stale DOM events.
    }
    const trimmed = pobText.trim();
    onSave({
      client_log_path: config.client_log_path,
      variant,
      pob_code: trimmed === "" ? null : trimmed,
      overlay_opacity: opacityPct / 100,
      hotkeys: normalizeHotkeyConfig(hotkeys),
    });
  }

  return (
    <div className="settings-page">
      {banner}
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

      <section className="settings-row">
        <label className="settings-label" htmlFor="opacity-slider">
          Overlay opacity
        </label>
        <div className="opacity-row">
          <input
            id="opacity-slider"
            className="opacity-slider"
            type="range"
            min={OPACITY_MIN_PCT}
            max={OPACITY_MAX_PCT}
            step={OPACITY_SLIDER_STEP_PCT}
            value={opacityPct}
            onChange={(e) => handleOpacityChange(Number(e.target.value))}
          />
          <span className="opacity-value">{opacityPct}%</span>
        </div>
      </section>

      <section className="settings-row">
        <span className="settings-label">Hotkeys</span>
        <p className="hotkey-hint">
          Global shortcuts, e.g. &quot;Alt+Shift+S&quot; — at least one modifier
          (Ctrl/Alt/Shift/Super) plus one key.
        </p>
        {HOTKEY_ACTIONS.map(({ key, label }) => (
          <div className="hotkey-row" key={key}>
            <label className="hotkey-label" htmlFor={`hotkey-${key}`}>
              {label}
            </label>
            <input
              id={`hotkey-${key}`}
              className={["hotkey-input", hotkeyErrors[key] && "hotkey-input-invalid"]
                .filter(Boolean)
                .join(" ")}
              type="text"
              value={hotkeys[key]}
              placeholder="e.g. Alt+Shift+S"
              onChange={(e) => handleHotkeyChange(key, e.target.value)}
            />
            {hotkeyErrors[key] && (
              <div className="hotkey-error">{hotkeyErrors[key]}</div>
            )}
          </div>
        ))}
      </section>

      <section className="settings-actions">
        <button
          type="button"
          className="btn btn-primary"
          onClick={handleSave}
          disabled={saving || hasHotkeyErrors}
        >
          {saving ? "Saving…" : "Save"}
        </button>
        {!saving && savedAt !== null && <span className="saved-confirmation">Saved &#10003;</span>}
      </section>
    </div>
  );
}
