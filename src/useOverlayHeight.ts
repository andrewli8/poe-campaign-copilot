import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef } from "react";
import { clampOverlayHeight } from "./overlayHeight";

export type SendHeight = (height: number) => void;

const defaultSend: SendHeight = (height) => {
  void invoke("set_overlay_height", { height }).catch((e) => {
    console.error("set_overlay_height failed:", e);
  });
};

// Returns a callback ref: attach it to the element whose height should drive
// the overlay window. A ResizeObserver measures that element and pushes its
// height to the backend `set_overlay_height` command, debounced and clamped,
// skipping a re-send when the clamped height is unchanged.
//
// A callback ref (not a passed-in RefObject) is deliberate: the overlay's
// root mounts only after `model` loads — several frames after this hook first
// runs — so an effect keyed on a ref object would run once with a null
// `.current` and never re-attach. React calls a callback ref when the node
// actually mounts and again with null when it unmounts, so the observer
// attaches exactly when the element appears. `send` is injectable for tests;
// production uses the Tauri invoke.
export function useOverlayHeight(
  opts: { send?: SendHeight; debounceMs?: number } = {},
): (node: HTMLElement | null) => void {
  const { send = defaultSend, debounceMs = 80 } = opts;

  // Latest opts held in refs so the callback ref keeps a stable identity
  // (React would otherwise detach/reattach the observer on every render).
  const sendRef = useRef(send);
  sendRef.current = send;
  const debounceRef = useRef(debounceMs);
  debounceRef.current = debounceMs;

  const observerRef = useRef<ResizeObserver | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastSentRef = useRef<number | null>(null);

  // React calls the callback ref with null on unmount (tearing down below),
  // but guard against a leaked timer if the component unmounts mid-debounce.
  useEffect(() => {
    return () => {
      observerRef.current?.disconnect();
      observerRef.current = null;
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, []);

  return useCallback((node: HTMLElement | null) => {
    // Detach from any previous node before (re)attaching.
    observerRef.current?.disconnect();
    observerRef.current = null;
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    lastSentRef.current = null;
    if (!node || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      const raw = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      const height = clampOverlayHeight(raw);
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        if (lastSentRef.current === height) {
          return;
        }
        lastSentRef.current = height;
        sendRef.current(height);
      }, debounceRef.current);
    });
    observer.observe(node);
    observerRef.current = observer;
  }, []);
}
