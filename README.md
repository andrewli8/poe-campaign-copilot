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

License: MIT (code). See CREDITS.md for third-party content.
