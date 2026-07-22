import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { SettingsPage } from "./SettingsPage";
import type { AppConfig, PobSummary } from "./types";

/// Thin, untested wiring layer (same split as useOverlay/FilmstripBar): all
/// `invoke` calls and settings-window-local state live here, so the
/// presentational `SettingsPage` stays a pure function of props.
///
/// `config` starts `null` and `SettingsPage` isn't mounted until the initial
/// `get_config` resolves — NOT seeded with an empty/default `AppConfig` up
/// front. `SettingsPage` seeds its local `variant`/`pobText` state from
/// `config` exactly once, on mount (see its own doc comment), so mounting it
/// early against a placeholder config would let those fields go stale and
/// silently clobber the real persisted values on Save. Gating the mount on
/// the loaded config sidesteps that resync problem entirely rather than
/// papering over it with an effect that re-seeds local state later.
export function SettingsContainer() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [preview, setPreview] = useState<PobSummary | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    let disposed = false;
    invoke<AppConfig>("get_config")
      .then((cfg) => {
        if (!disposed) setConfig(cfg);
      })
      .catch((e) => console.error("get_config failed:", e));
    return () => {
      disposed = true;
    };
  }, []);

  async function handlePick() {
    try {
      const path = await invoke<string | null>("pick_log_file");
      if (path) {
        setConfig((prev) => (prev ? { ...prev, client_log_path: path } : prev));
      }
    } catch (e) {
      console.error("pick_log_file failed:", e);
    }
  }

  async function handleImportPreview(code: string) {
    setPreviewError(null);
    setPreview(null);
    if (code.trim() === "") {
      return;
    }
    try {
      const summary = await invoke<PobSummary>("import_pob", { code });
      setPreview(summary);
    } catch (e) {
      setPreviewError(String(e));
    }
  }

  async function handleSave(cfg: AppConfig) {
    setSaving(true);
    try {
      await invoke("apply_settings", { cfg });
      setConfig(cfg);
      // A previous failed save (e.g. "log file not found") may have left
      // previewError set; a successful save supersedes it.
      setPreviewError(null);
      setSavedAt(Date.now());
    } catch (e) {
      // SettingsPage's props (fixed by the brief) only have one error slot —
      // previewError — so a failed save (e.g. "log file not found", a bad
      // PoB code re-validated server-side) surfaces there too rather than
      // via a second, unwired prop.
      setPreviewError(String(e));
    } finally {
      setSaving(false);
    }
  }

  if (config === null) {
    // Deliberately not rendering SettingsPage with a placeholder AppConfig
    // here — see the module doc comment above. Inline-styled (rather than
    // pulling in SettingsPage.css) since this placeholder is the one bit of
    // the settings window SettingsPage itself never renders.
    return (
      <div
        style={{
          minHeight: "100vh",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "#16110c",
          color: "#8a8072",
          fontFamily: "-apple-system, 'Segoe UI', system-ui, sans-serif",
          fontSize: 13,
        }}
      >
        Loading&hellip;
      </div>
    );
  }

  return (
    <SettingsPage
      config={config}
      onPick={handlePick}
      onImportPreview={handleImportPreview}
      preview={preview}
      previewError={previewError}
      onSave={handleSave}
      saving={saving}
      savedAt={savedAt}
    />
  );
}
