# Campaign Run Timer — Design

Date: 2026-07-23
Status: Approved

## Summary

An optional speedrun-style elapsed timer shown on the overlay. It starts
automatically at the first zone entry seen in the live Client.txt tail,
never auto-stops, survives app restarts, and is reset only via a new tray
menu item. It is shown by default and can be turned off with a checkbox in
Settings.

## Behavior

- Displays elapsed wall-clock time since run start, formatted `H:MM:SS`
  (hours unpadded, no cap: `12:03:09`, `0:00:42`).
- **Start:** the first area-entered line seen by the live tailer when no
  start is recorded sets the start to "now" and persists it. Journal replay
  at startup never sets the start — the persisted file from the original
  run is the source of truth.
- **Never auto-stops.** It keeps ticking after campaign completion; the
  chip simply shows elapsed time since start.
- **Reset:** a new tray menu item **Reset run timer** clears the persisted
  start. The overlay shows `0:00:00` and the next zone entry starts a new
  run.
- **Implicit reset:** the settings-apply path that clears the session
  journal (log path or route variant change — the `(d2)` path in
  `main.rs`) also clears the run timer. A new log or route means a new run.
- **Visibility:** shown in the normal header row, the zoom view (inherits
  the header), and the compact row. Hidden on the waiting-for-log screen
  and whenever `show_run_timer` is false.
- When enabled but not yet started, the chip shows `0:00:00`.

## Config

- `AppConfig.show_run_timer: bool`
  - Rust (`src-tauri/src/config.rs`): `#[serde(default = "default_show_run_timer")]`
    returning `true`, so pre-existing `config.json` files keep loading and
    get the timer on by default.
  - Included in `configs_equal`; **ignored** by `pipeline_configs_equal`
    (cosmetic setting like opacity/hotkeys — changing it must not restart
    the pipeline/tailer or clear the journal).
  - TypeScript mirror in `src/types.ts`.
- SettingsPage: a new "Run timer" section with a checkbox
  ("Show run timer on overlay"), edited locally and reported upward only on
  Save, like the other fields.

## Backend: `src-tauri/src/run_timer.rs` (new module)

- Persists the start as `run_timer.json` next to `config.json`:
  `{"started_at_ms": <u64 epoch millis>}`.
- API (mirroring `journal.rs` conventions):
  - `path(app) -> Option<PathBuf>` — `run_timer.json` in the app config
    dir; `None` degrades to no persistence.
  - `load(path) -> Option<u64>` — missing, unreadable, or corrupt file
    yields `None`, never a crash or error surfaced to the user.
  - `store(path, started_at_ms)` / `clear(path)` — IO errors are logged
    via `eprintln!` and otherwise ignored (timer keeps working in-memory
    for the session).
- In-memory state: `Mutex<Option<u64>>` alongside the other app state in
  `main.rs`.
- Start detection in the tail loop: if in-memory start is `None` and
  `event_parser::parse_line` recognizes the line as an area-entered event,
  set start to current system time, persist, and emit the `run-timer`
  event. Applies to all tailed lines including the `POE_COPILOT_LOG` dev
  override (persistence is still written; acceptable for a dev-only path).

## IPC and frontend

Follows the existing opacity pattern:

- Event `run-timer` (payload `number | null`, epoch ms) emitted on start
  and on reset.
- Event `show-run-timer` (payload `boolean`) emitted from `apply_settings`
  on save so the overlay window updates without restart.
- Initial snapshot: a `get_run_timer` Tauri command returns
  `Option<u64>`; `show_run_timer` rides the existing `get_config` fetch in
  `useOverlay`. Both use the established event-wins-over-snapshot pattern.
- `src/runTimer.ts` (new): pure `formatElapsed(elapsedMs: number): string`.
- Ticking: a 1-second `setInterval` re-render driver in `useOverlay`,
  active only while the timer is shown and started. No per-second IPC.
- `FilmstripBar`: new props for start-ms and visibility; renders the chip
  in the header row and compact row. Styling matches existing chips
  (`act-badge` / `town-chip` family) in `FilmstripBar.css`.

## Error handling

- All file IO on `run_timer.json` is non-fatal: log and continue.
- Corrupt/hand-edited JSON degrades to "not started".
- A persisted start in the future (clock skew) renders as `0:00:00`
  (negative elapsed clamps to zero in `formatElapsed`).

## Testing

- Rust `run_timer.rs`: store/load roundtrip; missing and corrupt file →
  `None`; clear removes the file.
- Rust `config.rs`: old config JSON without the field loads with
  `show_run_timer == true`; `configs_equal` distinguishes it;
  `pipeline_configs_equal` ignores it.
- Frontend `runTimer.test.ts`: formatting (zero, sub-minute, hour
  rollover, >10h, negative clamps to `0:00:00`).
- Frontend `FilmstripBar.test.tsx`: chip rendered when enabled+started,
  absent when disabled, `0:00:00` when enabled but not started, present in
  compact row.
- Frontend `SettingsPage.test.tsx`: checkbox reflects config and
  round-trips through Save.

## Out of scope (YAGNI)

- Pausing/freezing the timer (AFK detection, stop at campaign complete).
- Per-zone / split times.
- Automatic new-run detection (e.g. Twilight Strand re-entry).
- Hotkey for reset (tray only for now).
