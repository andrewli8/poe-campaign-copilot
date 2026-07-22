# PoE Campaign Copilot

A transparent Windows overlay for Path of Exile 1 campaign leveling
(Acts 1–10). Shows the current route step, zone layout images, and
build reminders — passively, from Client.txt only.

**Status: early development.**

## Content

- `content/layouts/` — per-zone layout notes and diagram images for all 10
  acts, keyed by exile-leveling area id, extracted from a community layout
  compilation (see CREDITS.md). Every entry carries audit metadata
  (`unaudited` → `verified`/`outdated`/`corrected`) so stale guidance is
  visible instead of silently wrong.
- `vendor/exile-leveling/` — pinned route + game data (MIT).
- `cargo run -p content --bin compile-content` builds the runtime content
  pack (routes + layouts + assets) into `content-pack/`.

- Passive-only: reads the game's `Client.txt` log. Never touches game
  memory, never simulates input, no network during play.
- English game clients only (for now).
- Route data based on [exile-leveling](https://github.com/HeartofPhos/exile-leveling) (MIT).
- Layout images by Engineering Eternity — see [CREDITS.md](CREDITS.md).

## Settings

The overlay reads its Client.txt path, route variant, and (optional) Path
of Building build from a small settings window, not from editing files by
hand:

1. Right-click the tray icon and choose **Settings…**.
2. **Client.txt log path** — click **Browse…** and pick your Path of
   Exile `Client.txt` (typically under
   `.../Path of Exile/logs/Client.txt` on Windows). This is required —
   the overlay shows a "Waiting for Client.txt…" state until a valid path
   is configured.
3. **Route variant** — `league-start` (a fresh character, the default) or
   `standard` (an existing character skipping early-game quests).
4. **Path of Building import (optional)** — paste a PoB share code (or
   raw XML export) to get build-specific reminders (e.g. "Gem available:
   Frostblink") alongside the route. Click **Preview import** first to
   see the parsed class/ascendancy, milestone count, and a reliability
   badge before saving — a `structured` badge means normalized gem/tree
   data was found; `unsupported` means the build parsed but yields no
   reminders (the route still works fine without one).
5. Click **Save**. This validates the settings, rebuilds the route
   pipeline for the new variant/build, restarts the log tailer at the new
   path if it changed, and persists everything to the app's config
   directory (`config.json`) for next launch.

Settings persist across restarts. The environment variables described
below (`POE_COPILOT_LOG`, `POE_COPILOT_LOG_REPLAY`) remain available as
developer-only overrides for local runs/demos — they take priority over
the configured path but are never written to `config.json`.

## Development

Rust workspace + Tauri 2. Built on macOS, validated on Windows.

    cargo test --workspace

The pilot test (`cargo test -p composer --test pilot_act1`) drives the full
pipeline — log fixture → session → route/task engines → composer — and is
the quickest way to see the whole system's behavior in one place.

### Try it: live overlay demo

Two terminals. The app tails a log file via `POE_COPILOT_LOG`; `fake-play`
simulates a play session by appending fixture lines to that file on a
delay, so you can watch the overlay react without the game running.

By default the tailer starts reading at end-of-file, since a real
`Client.txt` is a huge append-only history and replaying it all on launch
would be both slow and wrong. The demo below writes a *new*, empty log
file and then appends fixture lines to it, so it needs the tailer to
start reading from byte 0 instead — set `POE_COPILOT_LOG_REPLAY=1` to
opt into that for local development/demo runs only.

Terminal 1 — start the app pointed at a scratch log file:

    rm -f /tmp/fake-client.txt && touch /tmp/fake-client.txt
    POE_COPILOT_LOG=/tmp/fake-client.txt POE_COPILOT_LOG_REPLAY=1 npm run tauri dev

Wait for the Vite + Cargo build to finish; a transparent, always-on-top,
click-through bar appears reading "Waiting for Client.txt…".

Terminal 2 — replay a session (800ms between lines gives time to watch
each transition):

    cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 800

Expected: the bar transitions Waiting → Twilight Strand → Lioneye's Watch
→ The Coast (with layout images) → an off-route banner on the early town
revisit → Mud Flats.

Toggle Setup Mode (drag/resize) and Zoom from the tray icon menu, or zoom
with `alt+shift+z`. When done, stop the app (Ctrl-C in terminal 1 or quit
from the tray) and remove `/tmp/fake-client.txt`.

Point the app's log tailer at `/tmp/fake-client.txt` at any delay to watch
events flow without the two-terminal choreography above.

License: MIT (code). See CREDITS.md for third-party content.
