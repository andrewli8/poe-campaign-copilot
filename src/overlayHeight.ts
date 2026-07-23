// Clamp range for the content-driven overlay height, in logical pixels.
// Duplicated in src-tauri/src/main.rs (MIN/MAX_OVERLAY_HEIGHT) and as
// .filmstrip { max-height } in FilmstripBar.css — keep all three in step.
export const MIN_OVERLAY_HEIGHT = 36;
export const MAX_OVERLAY_HEIGHT = 600;

// Non-finite input (a stray NaN from a mid-layout measurement) collapses to
// the floor rather than propagating to the resize command.
export function clampOverlayHeight(height: number): number {
  if (!Number.isFinite(height)) {
    return MIN_OVERLAY_HEIGHT;
  }
  return Math.min(MAX_OVERLAY_HEIGHT, Math.max(MIN_OVERLAY_HEIGHT, height));
}
