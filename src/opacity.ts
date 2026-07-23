// Overlay opacity bounds, shared by the settings slider, the overlay
// window, and (mirrored in src-tauri/src/config.rs) the backend clamp.
// The floor exists so a user can never save/preview themselves into an
// invisible overlay they cannot find again.

export const OPACITY_MIN = 0.2;
export const OPACITY_MAX = 1;
export const OPACITY_DEFAULT = 1;

/** Percentage variants for the <input type="range"> slider. */
export const OPACITY_MIN_PCT = Math.round(OPACITY_MIN * 100);
export const OPACITY_MAX_PCT = Math.round(OPACITY_MAX * 100);

/**
 * Clamps an opacity to [OPACITY_MIN, OPACITY_MAX]; non-finite input (a
 * hand-edited config with `"overlay_opacity": "high"` arriving as NaN,
 * for instance) degrades to the default rather than to the floor, since
 * garbage carries no signal that the user wanted a dim overlay.
 */
export function clampOpacity(value: number): number {
  if (!Number.isFinite(value)) {
    return OPACITY_DEFAULT;
  }
  return Math.min(OPACITY_MAX, Math.max(OPACITY_MIN, value));
}
