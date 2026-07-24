# Development

PoE Campaign Copilot is a Rust workspace plus a Tauri 2 frontend. It is
developed on macOS; the release pipeline builds the Windows installer and the
Linux AppImage.

## Build from source

Install these three tools once:

1. **Rust** from [rustup.rs](https://rustup.rs). On Windows, accept the
   default MSVC toolchain; if it asks for the Visual Studio C++ Build Tools,
   let it install them.
2. **Node.js 20 or newer** from [nodejs.org](https://nodejs.org).
3. **Git** from [git-scm.com](https://git-scm.com).

On **Linux**, also install the system libraries Tauri needs (Debian/Ubuntu
shown; use your distro's equivalents on Fedora/Arch):

    sudo apt update
    sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
      libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
      patchelf xdg-utils

Then, in a terminal (PowerShell is fine on Windows):

    git clone https://github.com/andrewli8/poe-campaign-copilot.git
    cd poe-campaign-copilot
    npm install
    npm run tauri dev

The first build compiles a lot of Rust and can take 5 to 10 minutes. When
it finishes, a small transparent bar appears reading "Waiting for
Client.txt". Point it at your log and use it exactly as
[First-time setup](../README.md#first-time-setup) in the README describes.

The `POE_COPILOT_LOG` and `POE_COPILOT_LOG_REPLAY` environment variables
are developer overrides for demos. They beat the configured log path but are
never written to the config file.

## Testing status

The overlay has been developed and tested primarily on macOS with simulated
sessions, and the Windows and Linux builds compile clean in CI. Real-game
behavior — click-through over the game window, focus handling, global hotkeys
under XWayland on Linux, and log-line formats we haven't seen — is the area
most in need of testing. If something looks wrong, the terminal you launched
from usually says why.

## Tests

    cargo test --workspace

The pilot test drives the full pipeline from a log fixture through session,
engines, and composer. It is the quickest way to see the whole system's
behavior in one place:

    cargo test -p composer --test pilot_act1

## Live overlay demo (no game needed)

Two terminals. The app tails a scratch log file; `fake-play` appends
fixture lines to it on a delay, so you can watch the overlay react on any
OS.

The tailer normally starts reading at the *end* of the file, because a real
`Client.txt` is a huge append-only history and replaying it on launch would
be slow and wrong. The demo uses a fresh empty file, so it needs
`POE_COPILOT_LOG_REPLAY=1` to read from the start.

Terminal 1:

    rm -f /tmp/fake-client.txt && touch /tmp/fake-client.txt
    POE_COPILOT_LOG=/tmp/fake-client.txt POE_COPILOT_LOG_REPLAY=1 npm run tauri dev

Terminal 2, once the bar appears:

    cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 800

The bar walks through: Waiting, Twilight Strand, Lioneye's Watch, The Coast
(with layout images), an off-route banner on the early town revisit, then
Mud Flats. Stop the app with Ctrl-C or the tray's Quit, and delete
`/tmp/fake-client.txt` when done.

## Content pipeline

- `content/layouts/` has per-zone layout notes and diagram images for all
  10 acts, keyed by exile-leveling area id, extracted from a community
  layout compilation (see [CREDITS.md](../CREDITS.md)). Every note and image
  carries audit metadata (`unaudited`, `verified`, `outdated`, `corrected`)
  so stale guidance is visible instead of silently wrong.
- `vendor/exile-leveling/` holds pinned route and game data (MIT), from
  [exile-leveling](https://github.com/HeartofPhos/exile-leveling).
- Layout images are by Engineering Eternity. See [CREDITS.md](../CREDITS.md).
- Build the runtime content pack (routes, layouts, assets) into
  `content-pack/`:

      cargo run -p content --bin compile-content
