import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";

export type UpdaterStatus = "idle" | "checking" | "available" | "none" | "downloading" | "error";

/// Thin, untested wiring layer (same split as useOverlay/SettingsContainer):
/// all `@tauri-apps/plugin-updater` / `@tauri-apps/plugin-process` calls live
/// here, so `UpdateBanner` stays a pure function of props.
///
/// The check fires once, on mount. `SettingsContainer` mounts this hook when
/// the Settings window opens (see main.rs's `open_settings_window`), and
/// nowhere else — the main overlay window (`App.tsx`) never mounts it. That
/// is what satisfies the plan's "no network during play" constraint: the
/// only network call this app ever makes is this `check()`, and it only
/// happens when the user has explicitly opened Settings.
export function useUpdater() {
  const [status, setStatus] = useState<UpdaterStatus>("idle");
  const [version, setVersion] = useState<string | null>(null);
  const [progressPct, setProgressPct] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);

  useEffect(() => {
    let disposed = false;

    async function runCheck() {
      setStatus("checking");
      try {
        const result = await check();
        if (disposed) return;
        if (result) {
          setUpdate(result);
          setVersion(result.version);
          setStatus("available");
        } else {
          setStatus("none");
        }
      } catch (e) {
        if (disposed) return;
        setError(String(e));
        setStatus("error");
      }
    }

    runCheck();

    return () => {
      disposed = true;
    };
  }, []);

  async function installAndRestart() {
    if (!update) return;
    setStatus("downloading");
    setProgressPct(0);
    let contentLength: number | null = null;
    let downloaded = 0;
    try {
      await update.downloadAndInstall((event: DownloadEvent) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? null;
            downloaded = 0;
            setProgressPct(contentLength ? 0 : null);
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setProgressPct(contentLength ? Math.min(100, Math.round((downloaded / contentLength) * 100)) : null);
            break;
          case "Finished":
            setProgressPct(100);
            break;
        }
      });
      await relaunch();
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  return { status, version, progressPct, error, installAndRestart };
}
