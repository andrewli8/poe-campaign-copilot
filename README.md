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

Terminal 1 — start the app pointed at a scratch log file:

    rm -f /tmp/fake-client.txt && touch /tmp/fake-client.txt
    POE_COPILOT_LOG=/tmp/fake-client.txt npm run tauri dev

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
