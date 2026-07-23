# Campaign Run Timer — Design

Date: 2026-07-23
Status: Approved (rev 2: added configurable start/stop hotkey, pausable state)

## Summary

An optional speedrun-style elapsed timer shown on the overlay. It starts
automatically at the first zone entry seen in the live Client.txt tail,
can be paused/resumed with a configurable global hotkey, survives app
restarts, and is reset via a new tray menu item. It is shown by default
and can be turned off with a checkbox in Settings.

## Behavior

- Displays elapsed run time, formatted `H:MM:SS` (hours unpadded, no cap:
  `12:03:09`, `0:00:42`).
- The timer is a pausable stopwatch: elapsed = accumulated time from
  previous running stretches + (now − resume instant) while running.
- **Auto-start:** the first area-entered line seen by the live tailer when
  the timer is in the never-started state (no accumulated time, not
  running) starts it. Journal replay at startup never starts it — the
  persisted state file from the original run is the source of truth.
  Auto-start does NOT fire when the timer was manually paused: a paused
  timer stays paused across zone entries until resumed or reset.
- **Start/stop hotkey:** a configurable global hotkey (default
  `alt+shift+t`) toggles the timer — pauses it while running,
  resumes/starts it otherwise. Works even before the first zone entry.
- **Reset:** a new tray menu item **Reset run timer** returns the timer to
  the never-started state (`0:00:00`, not running). The next zone entry
  (or the hotkey) starts a new run.
- **Implicit reset:** the settings-apply path that clears the session
  journal (log path or route variant change — the `(d2)` path in
  `main.rs`) also resets the run timer. A new log or route means a new run.
- **Visibility:** shown in the normal header row, the zoom view (inherits
  the header), and the compact row. Hidden on the waiting-for-log screen
  and whenever `show_run_timer` is false. The timer keeps running while
  hidden — visibility is purely cosmetic.
- When shown but never started, the chip reads `0:00:00`. A paused timer
  shows its frozen elapsed time with a "paused" visual cue (dimmed chip
  plus a `⏸`-style marker).

## Config

- `AppConfig.show_run_timer: bool`
  - Rust (`src-tauri/src/config.rs`): `#[serde(default = "default_show_run_timer")]`
    returning `true`, so pre-existing `config.json` files keep loading and
    get the timer on by default.
  - Included in `configs_equal`; **ignored** by `pipeline_configs_equal`
    (cosmetic setting like opacity/hotkeys — changing it must not restart
    the pipeline/tailer or clear the journal).
  - TypeScript mirror in `src/types.ts`.
- `HotkeyConfig.timer: String`
  - Rust (`src-tauri/src/hotkeys.rs`): new field with
    `#[serde(default = "default_timer_hotkey")]` returning
    `"alt+shift+t"`, added to `HotkeyAction`, `bindings()` (now 6
    entries), and the dispatch match in `main.rs`.
  - TS (`src/hotkeys.ts`): added to `DEFAULT_HOTKEYS` and
    `HOTKEY_ACTIONS` (label: "Start/stop run timer"), so it appears in
    the existing hotkey settings UI with the same validation and
    conflict detection. `src/types.ts` mirror updated.
- SettingsPage: a new "Run timer" section with a checkbox
  ("Show run timer on overlay"), edited locally and reported upward only
  on Save, like the other fields. The hotkey row needs no new UI — it
  falls out of `HOTKEY_ACTIONS`.

## Backend: `src-tauri/src/run_timer.rs` (new module)

- State (also the persisted JSON shape, `run_timer.json` next to
  `config.json`):

  ```json
  { "accumulated_ms": 0, "running_since_ms": 1753280000000 }
  ```

  `running_since_ms` is null when paused/never-started. Never-started is
  `accumulated_ms == 0 && running_since_ms == null`.
- API (mirroring `journal.rs` conventions):
  - `path(app) -> Option<PathBuf>` — `None` degrades to no persistence.
  - `load(path) -> RunTimerState` — missing, unreadable, or corrupt file
    yields the never-started state, never a crash.
  - `store(path, &state)` / `clear(path)` — IO errors are logged via
    `eprintln!` and otherwise ignored (timer keeps working in-memory for
    the session).
  - Pure transitions (immutable: each returns a new state):
    `start(state, now_ms)`, `pause(state, now_ms)` (folds the running
    stretch into `accumulated_ms`), `toggle(state, now_ms)`,
    `elapsed_ms(state, now_ms)`.
- In-memory state: `Mutex<RunTimerState>` alongside the other app state
  in `main.rs`. Every transition persists and emits the `run-timer`
  event.
- Start detection in the tail loop: if the state is never-started and
  `event_parser::parse_line` recognizes the line as an area-entered
  event, apply `start(now)`. Applies to all tailed lines including the
  `POE_COPILOT_LOG` dev override (persistence is still written;
  acceptable for a dev-only path).
- Hotkey dispatch: the `timer` action applies `toggle(now)` via the same
  guarded-impl pattern as the other hotkey/tray actions.

## IPC and frontend

Follows the existing opacity pattern:

- Event `run-timer` (payload: the full `RunTimerState` as JSON) emitted
  on every transition (auto-start, hotkey toggle, reset).
- Event `show-run-timer` (payload `boolean`) emitted from
  `apply_settings` on save so the overlay window updates without restart.
- Initial snapshot: a `get_run_timer` Tauri command returns the state;
  `show_run_timer` rides the existing `get_config` fetch in `useOverlay`.
  Both use the established event-wins-over-snapshot pattern.
- `src/runTimer.ts` (new): TS mirror of `RunTimerState`, pure
  `elapsedMs(state, nowMs)` and `formatElapsed(elapsedMs): string`.
- Ticking: a 1-second `setInterval` re-render driver in `useOverlay`,
  active only while the timer is shown AND running (a paused or
  never-started timer needs no interval). No per-second IPC.
- `FilmstripBar`: new props for timer state and visibility; renders the
  chip in the header row and compact row, with the paused cue when
  `running_since_ms` is null but `accumulated_ms > 0`. Styling matches
  existing chips (`act-badge` / `town-chip` family) in `FilmstripBar.css`.

## Error handling

- All file IO on `run_timer.json` is non-fatal: log and continue.
- Corrupt/hand-edited JSON degrades to never-started.
- Clock skew: a `running_since_ms` in the future clamps that stretch to
  zero (negative elapsed never shown; `formatElapsed` clamps at
  `0:00:00`).
- Hotkey registration failure for the new chord follows the existing
  all-or-nothing `register_all` behavior and its existing error surface.

## Testing

- Rust `run_timer.rs`: store/load roundtrip; missing and corrupt file →
  never-started; clear removes the file; transition tests (start,
  pause folds elapsed into accumulated, toggle from each state,
  elapsed_ms while running/paused, future `running_since_ms` clamps).
- Rust `config.rs` / `hotkeys.rs`: old config JSON without the new
  fields loads with `show_run_timer == true` and
  `timer == "alt+shift+t"`; `configs_equal` distinguishes
  `show_run_timer`; `pipeline_configs_equal` ignores it; `validate`
  catches conflicts involving the new chord.
- Frontend `runTimer.test.ts`: `formatElapsed` (zero, sub-minute, hour
  rollover, >10h, negative clamps) and `elapsedMs` (running, paused,
  never-started, future-resume clamp).
- Frontend `hotkeys.test.ts`: `timer` present in defaults/actions and
  participates in conflict validation.
- Frontend `FilmstripBar.test.tsx`: chip rendered when enabled+running,
  absent when disabled, `0:00:00` when never-started, paused cue when
  paused, present in compact row.
- Frontend `SettingsPage.test.tsx`: checkbox reflects config and
  round-trips through Save; timer hotkey row renders and validates.

## Out of scope (YAGNI)

- AFK detection / auto-pause, freezing at campaign completion.
- Per-zone / split times.
- Automatic new-run detection (e.g. Twilight Strand re-entry).
- Reset via hotkey (tray only; the hotkey only toggles start/stop).
