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
  // Opens the file picker and resolves to the chosen path (or null if the
  // user cancelled). SettingsPage autosaves the picked path itself, so the
  // container only has to run the dialog.
  onPick: () => Promise<string | null> | void;
  onImportPreview: (code: string) => void;
  preview: PobSummary | null;
  previewError: string | null;
  // Persist the given config. Called automatically as fields are committed
  // (there is no Save button) — see the per-field commit points below.
  onSave: (cfg: AppConfig) => void;
  // Reset all campaign progress (route, reminders, level, timer) to a fresh
  // start.
  onReset: () => void;
  // Reset only the run timer to 0:00, independent of campaign progress.
  onResetTimer: () => void;
  saving: boolean;
  savedAt: number | null;
  // Live opacity preview: fired on every slider move with the fractional
  // opacity (0.2–1.0) so the container can push it to the overlay window
  // immediately, before the value is committed. Optional so presentational
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

/// Presentational settings form with autosave. Each field is edited in local
/// state and committed to `onSave` on its own natural trigger — discrete
/// controls (variant, run-timer, opacity release, log-path pick) on change,
/// free-text fields (PoB code, hotkeys) on blur — so a half-typed value never
/// reaches `apply_settings`. Invalid hotkeys are never committed. The full
/// `AppConfig` is always sent (built from `config` for the log path plus the
/// local field state), so the backend's no-op guard skips unchanged saves.
export function SettingsPage({
  config,
  onPick,
  onImportPreview,
  preview,
  previewError,
  onSave,
  onReset,
  onResetTimer,
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
  const [showRunTimer, setShowRunTimer] = useState(config.show_run_timer);
  const [confirmingReset, setConfirmingReset] = useState(false);

  const hotkeyErrors = validateHotkeyConfig(hotkeys);
  const hasHotkeyErrors = Object.keys(hotkeyErrors).length > 0;

  // Build the full config from the current field state; `partial` overrides a
  // just-changed field whose `setState` hasn't applied yet (setState is
  // async, so the changing handler passes its new value explicitly).
  function buildConfig(partial?: Partial<AppConfig>): AppConfig {
    const trimmed = pobText.trim();
    return {
      client_log_path: config.client_log_path,
      variant,
      pob_code: trimmed === "" ? null : trimmed,
      overlay_opacity: opacityPct / 100,
      hotkeys: normalizeHotkeyConfig(hotkeys),
      show_run_timer: showRunTimer,
      ...partial,
    };
  }

  // Autosave a committed change. Invalid hotkeys are never persisted (the
  // combo would fail to register server-side); the inline error stays until
  // the user fixes it.
  function commit(partial?: Partial<AppConfig>) {
    if (hasHotkeyErrors) {
      return;
    }
    onSave(buildConfig(partial));
  }

  function handleOpacityChange(pct: number) {
    setOpacityPct(pct);
    onOpacityPreview?.(pct / 100);
  }

  function handleHotkeyChange(action: keyof HotkeyConfig, value: string) {
    setHotkeys((prev) => ({ ...prev, [action]: value }));
  }

  async function handlePick() {
    const path = await onPick();
    if (path) {
      commit({ client_log_path: path });
    }
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
          <button type="button" className="btn btn-secondary" onClick={handlePick}>
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
          onChange={(e) => {
            const v = e.target.value as RouteVariant;
            setVariant(v);
            commit({ variant: v });
          }}
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
          onBlur={() => commit()}
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
            onPointerUp={() => commit()}
            onKeyUp={() => commit()}
          />
          <span className="opacity-value">{opacityPct}%</span>
        </div>
      </section>

      <section className="settings-row">
        <span className="settings-label">Run timer</span>
        <label className="checkbox-row" htmlFor="run-timer-checkbox">
          <input
            id="run-timer-checkbox"
            type="checkbox"
            checked={showRunTimer}
            onChange={(e) => {
              const checked = e.target.checked;
              setShowRunTimer(checked);
              commit({ show_run_timer: checked });
            }}
          />
          Show run timer on overlay
        </label>
        <div className="run-timer-actions">
          <button type="button" className="btn btn-secondary" onClick={onResetTimer}>
            Reset run timer
          </button>
        </div>
      </section>

      <section className="settings-row">
        <span className="settings-label">Hotkeys</span>
        <p className="hotkey-hint">
          Global shortcuts, e.g. &quot;Alt+Shift+S&quot; — at least one modifier
          (Ctrl/Alt/Shift/Super) plus one key. Invalid combos are not saved.
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
              onBlur={() => commit()}
            />
            {hotkeyErrors[key] && (
              <div className="hotkey-error">{hotkeyErrors[key]}</div>
            )}
          </div>
        ))}
      </section>

      <section className="settings-row">
        <span className="settings-label">Reset progress</span>
        {confirmingReset ? (
          <div className="reset-confirm">
            <span className="reset-warning">
              Reset all campaign progress to a fresh start? This clears the
              current route position and can&rsquo;t be undone.
            </span>
            <div className="reset-confirm-actions">
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => {
                  setConfirmingReset(false);
                  onReset();
                }}
              >
                Yes, reset
              </button>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => setConfirmingReset(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => setConfirmingReset(true)}
          >
            Reset progress&hellip;
          </button>
        )}
      </section>

      <section className="settings-actions">
        {saving ? (
          <span className="autosave-status">Saving&hellip;</span>
        ) : savedAt !== null ? (
          <span className="autosave-status saved">Saved &#10003;</span>
        ) : (
          <span className="autosave-status muted">Changes save automatically</span>
        )}
      </section>
    </div>
  );
}
