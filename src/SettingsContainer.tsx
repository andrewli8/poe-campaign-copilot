import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { SettingsPage } from "./SettingsPage";
import type { AppConfig, PobSummary } from "./types";

const EMPTY_CONFIG: AppConfig = {
  client_log_path: null,
  variant: "league-start",
  pob_code: null,
};

/// Thin, untested wiring layer (same split as useOverlay/FilmstripBar): all
/// `invoke` calls and settings-window-local state live here, so the
/// presentational `SettingsPage` stays a pure function of props.
export function SettingsContainer() {
  const [config, setConfig] = useState<AppConfig>(EMPTY_CONFIG);
  const [preview, setPreview] = useState<PobSummary | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<number | null>(null);

  useEffect(() => {
    let disposed = false;
    invoke<AppConfig>("get_config").then((cfg) => {
      if (!disposed) setConfig(cfg);
    });
    return () => {
      disposed = true;
    };
  }, []);

  async function handlePick() {
    const path = await invoke<string | null>("pick_log_file");
    if (path) {
      setConfig((prev) => ({ ...prev, client_log_path: path }));
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
