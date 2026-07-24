// Clamp range for the content-driven overlay height, in logical pixels.
// Duplicated in src-tauri/src/main.rs (MIN/MAX_OVERLAY_HEIGHT) and as
// .filmstrip { max-height } in FilmstripBar.css — keep all three in step.
export const MIN_OVERLAY_HEIGHT = 36;
export const MAX_OVERLAY_HEIGHT = 600;

// Non-finite input (a stray NaN from a mid-layout measurement) collapses to
// the floor rather than propagating to the resize command.
//
// Fractional measurements are rounded UP to a whole logical pixel. The
// window must never end up fractionally smaller than the content: native
// window heights land on physical pixels, and a sub-pixel shortfall makes
// the document overflow its viewport. On Windows that overflow grows a
// classic, layout-consuming scrollbar, which reflows the content to a new
// height, re-fires the ResizeObserver, and resizes the window again — a
// sustained resize/reflow loop that recomposites the desktop at the
// debounce cadence and tanks the frame rate of a game running underneath.
export function clampOverlayHeight(height: number): number {
  if (!Number.isFinite(height)) {
    return MIN_OVERLAY_HEIGHT;
  }
  return Math.min(MAX_OVERLAY_HEIGHT, Math.max(MIN_OVERLAY_HEIGHT, Math.ceil(height)));
}
