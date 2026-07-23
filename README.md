# PoE Campaign Copilot

A transparent, passive overlay for Path of Exile 1 campaign leveling (Acts
1–10). It shows your current route step, zone layout diagrams, and build
reminders — reading only the game's `Client.txt` log. No game memory, no
input simulation, no network while you play.

**Early development · English game clients only for now.**

<img width="1384" height="582" alt="poedemostatic" src="https://github.com/user-attachments/assets/ecfb6b14-2281-4d38-8461-3bee54ed4a5f" />

## Install (Windows)

1. Download the latest `poe-campaign-copilot-*-setup.exe` from the
   [releases page](https://github.com/andrewli8/poe-campaign-copilot/releases).
2. Run it — installs per-user, no admin prompt.
3. If SmartScreen warns that it's unsigned, click **More info → Run anyway**.
   (Code signing is future work.)
4. Launch **PoE Campaign Copilot** from the Start menu.

Prefer to build from source? See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## First-time setup

1. Right-click the tray icon → **Settings**.
2. **Browse** to your `Client.txt` log:
   - Steam — `…\steamapps\common\Path of Exile\logs\Client.txt`
   - Standalone — `…\Grinding Gear Games\Path of Exile\logs\Client.txt`
3. *(Optional)* paste a Path of Building share code — see [Settings](#settings).
4. Click **Save**.

Run Path of Exile in **windowed fullscreen** — an overlay can't draw over
exclusive fullscreen. The bar reads from the *end* of the log, so it shows
"Waiting for Client.txt…" until your first zone change. Enter any area and
it comes alive.

## Controls

The common toggles have global hotkeys; all of them are also on the tray
menu (right-click the tray icon).

| Action | Hotkey |
| --- | --- |
| Setup mode — drag and resize the bar | `Alt+Shift+S` |
| Zoom — larger layout images | `Alt+Shift+Z` |
| Compact — slimmer bar | `Alt+Shift+C` |
| Hide / show the overlay | `Alt+Shift+H` |
| Open Settings | `Alt+Shift+O` |
| Start / stop the run timer | `Alt+Shift+T` |

The tray menu additionally has **Reset run timer**, **Open logs** (see
Reporting bugs below), and **Quit**.

**Off-route tracking.** If you head into a zone you skipped or one earlier
than your furthest point, the overlay follows you instead of insisting you
turn back. It labels what's happening with a chip — **Catching up** for a
zone you passed over, **Revisiting** for one you'd already cleared — plus a
line showing how many zones behind your furthest point you are. Your actual
route progress is never changed.

## Settings

Open with the tray icon → **Settings**. The main options:

- **Client.txt log path** *(required)* — the overlay waits until this is set.
- **Route variant** — `league-start` (default; assumes a fresh character) or
  `standard` (skips the league-start-only steps).
- **Path of Building import** *(optional)* — paste a share code or XML export
  and click **Preview import** to see the class, ascendancy, and a
  reliability badge before saving. A `structured` badge means the build has
  parseable leveling data, so you'll get in-town reminders like "Gem
  available: Frostblink"; `unsupported` means it parsed but had no timing
  data — the route still works without it.

Settings also offers overlay opacity, customizable hotkeys, and a toggle to
show or hide the run timer. Saving persists to the app's config directory
and survives restarts; saving a change restarts route tracking from scratch.

## Updating

From 0.1.2 on, the app checks for a newer release whenever you open Settings.
If one is available, a banner offers **Update and restart** — one click
downloads, installs, and relaunches on its own. The check runs only when
Settings is open, never during play (so it keeps the "no network while you
play" rule), and every update is cryptographically signed. Versions 0.1.0
and 0.1.1 predate the updater, so install 0.1.2 by hand once from the
releases page; every version after that updates in place.

## Zone layout images

When you enter a zone, the filmstrip shows small hand-drawn cheat-sheet
sketches — one per common spawn variant — each abstracting the zone into a
gray footprint plus a suggested path, so you can match it against your
in-game map at a glance. They're community sketches by
[Engineering Eternity](https://www.youtube.com/@EngineeringEternity) (see
[CREDITS.md](CREDITS.md)), from an older patch, so treat them as strong
hints rather than guarantees.

![Layout legend: 1 entrance, 2 waypoint, 3 exit and main path, 4 trial and path, 5 optional area and path, E initial of important NPC](docs/images/layout-legend.png)

**→ Full guide to reading the diagrams: [docs/LAYOUTS.md](docs/LAYOUTS.md).**

## Reporting bugs

Found something broken or wrong? Please open an issue at
[github.com/andrewli8/poe-campaign-copilot/issues](https://github.com/andrewli8/poe-campaign-copilot/issues).

The overlay keeps a small local log of what it's doing and any errors it
hits, including crashes. Grab it before you file: right-click the tray icon
and choose **Open logs**, then attach `poe-copilot.log`. It records the app
version, the zones it detected, and error messages, which makes a bug far
easier to track down. It stays on your machine and only contains local
diagnostics, never account or personal data.

When reporting, it helps to include: what you were doing, what you expected
versus what happened, your Windows version, and whether you run Steam or
standalone Path of Exile.

## Building from source & contributing

Build steps, the test suite, and a live demo harness that drives the overlay
without the game are in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). The
layout/route data sources and content pipeline are covered there too.

## Credits & license

Code is MIT. Route and game data come from
[exile-leveling](https://github.com/HeartofPhos/exile-leveling) (MIT); layout
notes and images are community content by Engineering Eternity. Full
attributions in [CREDITS.md](CREDITS.md).
