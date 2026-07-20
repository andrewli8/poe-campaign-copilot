# PoE Campaign Copilot

A transparent Windows overlay for Path of Exile 1 campaign leveling
(Acts 1–10). Shows the current route step, zone layout images, and
build reminders — passively, from Client.txt only.

**Status: early development.**

- Passive-only: reads the game's `Client.txt` log. Never touches game
  memory, never simulates input, no network during play.
- English game clients only (for now).
- Route data based on [exile-leveling](https://github.com/HeartofPhos/exile-leveling) (MIT).
- Layout images by Engineering Eternity — see [CREDITS.md](CREDITS.md).

## Development

Rust workspace + Tauri 2. Built on macOS, validated on Windows.

    cargo test --workspace

License: MIT (code). See CREDITS.md for third-party content.
