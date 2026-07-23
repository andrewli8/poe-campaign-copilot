import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import type { RefObject } from "react";
import { clampOverlayHeight } from "./overlayHeight";

export type SendHeight = (height: number) => void;

const defaultSend: SendHeight = (height) => {
  void invoke("set_overlay_height", { height });
};

// Measures `ref`'s rendered height with a ResizeObserver and pushes it to
// the backend `set_overlay_height` command, debounced and clamped. Skips
// the call when the clamped height matches the last one sent, so a settle
// that lands on the same value does not spam IPC. `send` is injectable for
// tests; production uses the Tauri invoke.
export function useOverlayHeight(
  ref: RefObject<HTMLElement | null>,
  opts: { send?: SendHeight; debounceMs?: number } = {},
): void {
  const { send = defaultSend, debounceMs = 80 } = opts;

  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") {
      return;
    }

    let lastSent: number | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      const raw = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      const height = clampOverlayHeight(raw);
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = null;
        if (lastSent === height) {
          return;
        }
        lastSent = height;
        send(height);
      }, debounceMs);
    });

    observer.observe(el);
    return () => {
      observer.disconnect();
      if (timer !== null) {
        clearTimeout(timer);
      }
    };
  }, [ref, send, debounceMs]);
}
