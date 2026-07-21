import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { UiModel } from "./types";

export function useOverlay() {
  const [model, setModel] = useState<UiModel | null>(null);
  const [zoom, setZoom] = useState(false);
  const [setupMode, setSetupMode] = useState(false);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    invoke<UiModel>("get_model").then((m) => {
      if (!disposed) setModel(m);
    });
    listen<UiModel>("overlay-model", (e) => setModel(e.payload)).then((u) => unlisteners.push(u));
    listen<boolean>("zoom", (e) => setZoom(e.payload)).then((u) => unlisteners.push(u));
    listen<boolean>("setup-mode", (e) => setSetupMode(e.payload)).then((u) => unlisteners.push(u));
    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, []);

  return { model, zoom, setupMode };
}
