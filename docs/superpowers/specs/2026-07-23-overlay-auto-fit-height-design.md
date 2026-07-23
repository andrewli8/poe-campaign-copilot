# Overlay auto-fit height

**Date:** 2026-07-23
**Status:** Approved, pending implementation

## Problem

The overlay window has a fixed height per mode (44px compact, 150px normal,
420px zoom), set by `overlay_target_height()` in `src-tauri/src/main.rs`. The
content it renders is dynamic: a zone with three layout images and several
notes is much taller than a bare town reminder, and milestone lists grow and
shrink as the player levels. When content is taller than the current mode's
fixed height it gets clipped, and the only recourse is Setup Mode manual
resizing — which the user then has to redo every time the content changes.

The goal: the window height should follow the content automatically, so a
user never has to resize to read clipped information. Width and screen
position stay user-controlled.

## Approach

**Content-driven height.** The frontend measures its own rendered height and
tells the backend to size the window to match. A `ResizeObserver` on the
filmstrip root fires whenever the rendered height changes (mode toggles, new
zone, milestone list growth, image load). A debounced handler sends the
measured logical height to a new `set_overlay_height` Tauri command, which
resizes the `main` window to `(current width, measured height)`.

This replaces the fixed per-mode heights. Compact and zoom toggles no longer
resize the window directly — they change what is rendered, the observer sees
the new height, and the window follows.

### Grow anchor

The top edge stays pinned; the window grows and shrinks downward. This is the
current `set_size` behavior (position unchanged, only height changes), so no
extra positioning logic is needed. Users who park the bar very low on screen
may see tall content extend toward the bottom edge; the internal scroll cap
(below) bounds how far that can go.

### Bounds

Measured height is clamped to `[MIN_OVERLAY_HEIGHT, MAX_OVERLAY_HEIGHT]` =
`[36, 600]` logical px. When content exceeds the ceiling — e.g. zoom
mode with tall images plus a long notes list — the window holds at the cap and
the filmstrip scrolls internally via `overflow-y: auto` on the root, so no
information is ever unreachable. The backend command re-validates the range,
since IPC is an untrusted boundary; out-of-range or non-finite values are
rejected and logged rather than applied.

### Debounce

The observer can fire many times in quick succession (image decode, React
re-render, font load). A trailing debounce of ~80ms coalesces bursts into a
single `set_overlay_height` call, so the window resizes once per settle rather
than flickering through intermediate heights.

## Components

- **`src/useOverlayHeight.ts` (new hook).** Takes a ref to the measured
  element. Sets up a `ResizeObserver`, debounces, clamps to
  `[MIN, MAX]`, and calls `invoke("set_overlay_height", { height })`. Cleans
  up the observer and any pending timer on unmount. Skips the invoke when the
  clamped height is unchanged from the last sent value, to avoid redundant IPC.
- **`src/FilmstripBar.tsx`.** Attaches the hook's ref to the `.filmstrip`
  root. No structural changes to what it renders.
- **`src/FilmstripBar.css`.** Add `max-height: 600px; overflow-y: auto` to
  `.filmstrip` so over-cap content scrolls inside the window instead of being
  clipped by it. The shared `MAX_OVERLAY_HEIGHT` value (600) is duplicated
  between this CSS cap and the hook's clamp; a comment in each ties them
  together so they stay in step.
- **`src-tauri/src/main.rs`.**
  - New command `set_overlay_height(height: f64)` — validates
    `height.is_finite()` and `MIN <= height <= MAX`, then
    `win.set_size(LogicalSize::new(current_logical_width, height))`.
  - Delete `overlay_target_height()`.
  - Remove the `set_size` blocks from `toggle_zoom_impl` and
    `toggle_compact_impl`; they keep only the flag flip, tray-check sync, and
    event emit.
  - Register the new command in the `invoke_handler`.

## Data flow

```
content changes (mode toggle / new zone / milestones / image load)
        │
        ▼
ResizeObserver fires on .filmstrip
        │  (debounce ~80ms, clamp to [MIN,MAX], skip-if-unchanged)
        ▼
invoke("set_overlay_height", { height })
        │
        ▼
set_overlay_height validates → win.set_size(width, height)
        │
        ▼
window height matches content; top edge pinned
```

## Concurrency note

The documented zoom↔compact lock-ordering guard exists solely to keep the two
`set_size` paths in `toggle_zoom_impl` / `toggle_compact_impl` from
interleaving and leaving the window size out of sync with the flags. Once both
resize paths are removed and height is driven only by the observer, that hazard
is gone. The `zoom` and `compact` mutexes remain for their flag state, but the
functions no longer both need to read-the-other-then-lock; the cross-lock
comments are removed with the resize code. (The flags still gate rendering, so
they stay.)

## Error handling

- **IPC boundary.** `set_overlay_height` rejects non-finite or out-of-range
  heights (returns `Result::Err`, logged via `eprintln!`), never resizing to a
  garbage value.
- **Missing window.** If `get_webview_window("main")` is `None` (shutdown
  race), the command is a no-op returning `Ok`.
- **Observer absent.** If `ResizeObserver` is unavailable (should never happen
  in the Tauri webview, but guards tests), the hook no-ops and the window keeps
  its last size rather than throwing.

## Testing

- **Rust unit tests** on `set_overlay_height` validation: finite/in-range
  applies, NaN/infinite/below-min/above-max rejected. Existing compact/zoom
  toggle tests lose their size assertions and assert only flag + event
  behavior.
- **Frontend tests** (`useOverlayHeight`) with a mocked `ResizeObserver` and
  mocked `invoke`:
  - Debounce: three rapid size changes → one invoke with the final value.
  - Clamp: a measured height above MAX → invoke receives MAX; below MIN →
    MIN.
  - Skip-if-unchanged: same clamped height twice → invoke called once.
  - Cleanup: unmount clears the pending timer and disconnects the observer.
- **Manual smoke** via the fake-play demo: enter a zone with images, toggle
  zoom and compact, confirm the window tracks height with no clipping and no
  flicker, and that over-cap content scrolls internally.

## Out of scope

- Width auto-fit — width stays user-controlled in Setup Mode.
- Grow-away-from-screen-edge anchoring — top-pinned only, per decision.
- Animated resize transitions — resize is immediate.
