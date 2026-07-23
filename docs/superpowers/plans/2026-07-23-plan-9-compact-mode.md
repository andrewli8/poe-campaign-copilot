# Plan 9: Compact Overlay Mode

**Goal:** A "compact" toggle that shrinks the overlay to a slim single line showing only the essential next plan of action — zone name, the primary action, and the next zone — hiding images, notes, reminders, and badges. Toggled from the tray and `Alt+Shift+C`, like the existing Zoom/Hide toggles.

**Architecture:** Mirrors the established toggle pattern in `src-tauri/src/main.rs` (Setup Mode / Zoom / Hide): a `compact` bool + a stored `CheckMenuItem` handle in `AppState`, a `toggle_compact_impl` helper that resizes the window and syncs the tray check + emits a `compact` event, a tray menu item, a global shortcut, and a `toggle_compact` command. The frontend `useOverlay` hook gains a `compact` boolean fed from the event; `FilmstripBar` renders a compact branch when it's on.

## Global Constraints
- Compact affects DISPLAY only — the route/session pipeline is untouched; the same `OverlayModel` drives both modes.
- Compact shows exactly: `zone_name`, `primary`, and `→ {next_zone}` (when `next_zone` is set). Nothing else (no images, notes, sub-hints, build/town reminders, pending badge, off-route banner). In the `waiting_for_log` and `route_complete` states, compact renders the same minimal status those states already show (waiting pill / "Campaign complete") — compact only strips the normal playing layout.
- Window sizing is now a function of (compact, zoom): compact → slim (~44 logical px height); else zoom → 420; else → 150. Both `toggle_zoom_impl` and `toggle_compact_impl` must land on the correct height given the other's current state (compact takes precedence over zoom for height). Keep width unchanged.
- Draggable-in-setup-mode still works in compact (the root remains the drag region).
- Compact defaults OFF at launch (event-driven state, like zoom/setup/hidden). Persisting it is out of scope.
- Edition 2024; commit format; no AI attribution.

## Task 1: Rust toggle + tray + shortcut + window sizing

**Files:** `src-tauri/src/main.rs`, `src-tauri/src/main.rs` tray/shortcut sections.

- [ ] Add to `AppState`: `compact: Mutex<bool>` (default false) and `compact_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>` (None), initialized at the single construction site.
- [ ] Extract window-height logic so both toggles agree: a helper `fn overlay_target_height(compact: bool, zoom: bool) -> f64 { if compact { 44.0 } else if zoom { 420.0 } else { 150.0 } }`. Refactor `toggle_zoom_impl` to compute its target via this helper (reading the current `compact` flag) rather than the hardcoded `if new_zoom { 420 } else { 150 }` — so toggling zoom while compact is on doesn't fight compact. Preserve `toggle_zoom_impl`'s existing lock-across-resize discipline and its `zoom` event emit.
- [ ] Add `toggle_compact_impl(app) -> bool` mirroring `toggle_zoom_impl`: flip `compact` (guard held across the resize), resize the "main" window via LogicalSize using `overlay_target_height(new_compact, current_zoom)` and the current logical width (same scale-factor conversion as zoom), sync the stored `compact_item` checkmark, emit a `compact` event with the new bool. Handle window-op errors with `eprintln!` like the siblings.
- [ ] Add a `#[tauri::command] fn toggle_compact(app) -> bool` calling the helper (parity with `toggle_zoom`); register it in the invoke handler.
- [ ] Tray: add a `CheckMenuItem` id `"compact"` labeled "Compact mode", menu order Setup Mode → Zoom → Compact mode → Hide overlay → Settings… → Quit; store its handle in `AppState.compact_item`; add a `"compact" => { toggle_compact_impl(app); }` handler arm.
- [ ] Global shortcut: add `alt+shift+c`; in the shortcut handler add an `else if shortcut.matches(alt_shift, Code::KeyC) { toggle_compact_impl(app); }` branch alongside the existing z/h branches.
- [ ] Gate: `cargo test --workspace`, fmt, clippy green; `npm run build`; brief mac `tauri dev` smoke (tray builds with the new item, no panic). Commit: `feat: compact overlay mode toggle (tray + alt+shift+c)`

## Task 2: Frontend compact rendering

**Files:** `src/useOverlay.ts`, `src/App.tsx`, `src/FilmstripBar.tsx`, `src/FilmstripBar.css`, `src/FilmstripBar.test.tsx`

- [ ] `useOverlay`: subscribe to the `compact` event (like `zoom`/`setup-mode`), return `compact: boolean` (default false). Register/cleanup with the same disposal-safe pattern as the other listeners.
- [ ] `App.tsx`: pass `compact` to `FilmstripBar`.
- [ ] `FilmstripBar`: add `compact: boolean` to props; add `compact && "compact"` to `rootClass`. In the normal (non-waiting, non-complete) branch, when `compact` is true render ONLY a compact row: `<div className="compact-row">` containing `zone-name` (zone_name), `compact-primary` (primary), and, when `next_zone`, a `compact-next` (`→ {next_zone}`). Skip build-summary, header-row, off-route banner, images, notes, sub-hints, reminders. The root keeps `data-tauri-drag-region={setupMode ? true : undefined}`. Waiting/complete branches render as they do today regardless of compact.
- [ ] `FilmstripBar.css`: `.filmstrip.compact` slim styling (single row, reduced padding, `zone-name` gold, `compact-primary` white, `compact-next` muted `#8a94a8`), consistent with the existing bar aesthetic.
- [ ] Tests (`FilmstripBar.test.tsx`): compact=true renders zone/primary/next and does NOT render the image row / notes / pending badge (assert absence of a known full-mode element, e.g. `.image-row` or the "images" count); compact=false unchanged; compact + setupMode still has the root drag region. Update the shared `model()` helper / render calls to pass the new `compact` prop (default false) so existing tests are unaffected.
- [ ] Gate: `npm test`, `npm run build`, full Rust gate green. Commit: `feat: compact overlay rendering (zone, action, next zone)`

## Verification
- [ ] Gates green (Rust + vitest).
- [ ] Toggling compact from tray/hotkey shrinks the bar to the slim row and back; zoom/compact heights don't fight.
- [ ] Compact bar still draggable in setup mode.

## Self-Review Notes
- Reuses the exact Zoom/Hide toggle machinery; the only new cross-cutting concern is window height now depending on two flags, handled by the shared `overlay_target_height` helper so the two toggles can't disagree.
- Display-only; no pipeline/model changes; the same OverlayModel renders both modes.
