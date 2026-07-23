import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { OPACITY_DEFAULT, clampOpacity } from "./opacity";
import type { AppConfig, UiModel } from "./types";

export function useOverlay() {
  const [model, setModel] = useState<UiModel | null>(null);
  const [zoom, setZoom] = useState(false);
  const [setupMode, setSetupMode] = useState(false);
  const [compact, setCompact] = useState(false);
  const [overlayOpacity, setOverlayOpacity] = useState(OPACITY_DEFAULT);

  useEffect(() => {
    let disposed = false;
    let eventModelArrived = false;
    let opacityEventArrived = false;
    const unlisteners: UnlistenFn[] = [];

    // Registers a listener and makes sure it is always torn down exactly
    // once: if disposal (StrictMode double-mount, or a real unmount)
    // happens before the `listen()` promise resolves, unsubscribe as soon
    // as it resolves instead of stashing the unlisten fn for a cleanup
    // that already ran (which would leak the subscription).
    function registerListener<T>(event: string, onPayload: (payload: T) => void) {
      const promise = listen<T>(event, (e) => onPayload(e.payload));
      promise.then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      });
      return promise;
    }

    async function setup() {
      // Register all listeners first, and wait for them to be live,
      // before asking for the current model. Otherwise an overlay-model
      // event emitted between the invoke() call and the listener actually
      // being registered would be silently missed.
      const listenersReady = Promise.all([
        registerListener<UiModel>("overlay-model", (m) => {
          eventModelArrived = true;
          setModel(m);
        }),
        registerListener<boolean>("zoom", (z) => setZoom(z)),
        registerListener<boolean>("setup-mode", (s) => setSetupMode(s)),
        registerListener<boolean>("compact", (c) => setCompact(c)),
        registerListener<number>("overlay-opacity", (o) => {
          opacityEventArrived = true;
          setOverlayOpacity(clampOpacity(o));
        }),
      ]);
      await listenersReady;
      if (disposed) return;

      try {
        const initial = await invoke<UiModel>("get_model");
        // Only apply the invoke() result if no overlay-model event arrived
        // while we were waiting on it — an event always wins because it
        // reflects a newer state than the snapshot we requested.
        if (!disposed && !eventModelArrived) {
          setModel(initial);
        }
      } catch (e) {
        console.error("get_model failed:", e);
      }

      // Startup opacity comes from the persisted config; a live
      // "overlay-opacity" event (settings slider preview / Save) that
      // arrived while we were fetching always wins over this snapshot.
      try {
        const cfg = await invoke<AppConfig>("get_config");
        if (!disposed && !opacityEventArrived) {
          setOverlayOpacity(clampOpacity(cfg.overlay_opacity));
        }
      } catch (e) {
        console.error("get_config failed:", e);
      }
    }

    setup();

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  return { model, zoom, setupMode, compact, overlayOpacity };
}
