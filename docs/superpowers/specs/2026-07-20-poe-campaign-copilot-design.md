# PoE Campaign Copilot — Design Spec

**Date:** 2026-07-20
**Status:** Approved for implementation
**Supersedes:** PRD/Architecture v1.0 (GO) where noted below

## 1. Product Scope

A transparent desktop overlay for Path of Exile 1 campaign leveling.

- **Acts 1–10**, one standard fast route (exile-leveling based).
- **English game clients only.** Area detection keys on English Client.txt lines.
- **Windows 10/11 is the runtime target.** Development happens on macOS; the app
  also runs on macOS for development (overlay behaves as a normal window there).
- Priorities: **working and accurate** over metrics. The v1.0 PRD's pilot-gate
  statistics (8% improvement targets, confirmation-count budgets) are dropped.
  Accuracy auditing (provenance, patch status, "tip wrong" marker) is retained.
- **Passive-only boundary (unchanged from PRD):** read the user-selected
  Client.txt and PoB files only. No process memory access, no injection, no
  rendering hooks, no packet inspection, no input simulation, no gameplay
  automation. No network requests while a play session is active.

### Non-goals

- Exact player-position tracking or live navigation.
- Automatic completion detection for pickups, optional bosses, purchases,
  sockets, or passives (soft/pending task model instead).
- Non-English clients, PoE2, console.

## 2. Architecture

**Single Tauri 2 application** (revised from PRD ADR-001's native Win32 HUD;
revision driven by macOS-first development).

- **Rust core crates** (cross-platform, no UI dependencies):
  - `input_log` — discover/tail Client.txt; handle truncation, rotation,
    replacement, restarts, partial lines.
  - `event_parser` — typed events (`AreaEntered`, `TownEntered`, level-ups,
    `SessionStarted/Ended`, `UnknownLogLine`) from English log lines.
  - `session` — zone/act/town state, timestamps, lifecycle.
  - `route_engine` — active route step + legal transitions from compiled route
    data.
  - `task_engine` — task states: Upcoming, Active, Confirmed, Inferred,
    Unresolved, Skipped, Invalidated. Evidence-based, reversible inference.
  - `pob_import` — PoB share code (base64 + zlib) and XML → normalized
    `LevelingBuildPlan` with reliability classes (Explicit / Structured /
    Inferred / Unsupported → route-only fallback).
  - `content` — load/validate content packs (JSON Schema, checksums, patch
    pinning, audit status).
  - `composer` — immutable display models for the overlay bar and zoom view.
- **UI (React/TypeScript in Tauri webviews):**
  - Overlay window — the filmstrip bar (Section 5).
  - Settings window — setup, PoB import, diagnostics, content audit review.
- **Windows overlay flags** applied at runtime: always-on-top, transparent,
  click-through (`set_ignore_cursor_events(true)`), no-activate/no-focus on
  show, per-monitor DPI awareness. On macOS these are best-effort; the window
  behaves normally for development.
- **Accepted trade-offs vs. the PRD:** memory ~100–150 MB (PRD targeted 50 MB);
  web-rendered rather than Direct2D. Event-to-HUD latency target stays
  p95 < 400 ms. Approach is proven by existing PoE overlays (e.g., Awakened
  PoE Trade).

## 3. Route Data (exile-leveling)

- Vendor `act-1.txt` … `act-10.txt` from
  [HeartofPhos/exile-leveling](https://github.com/HeartofPhos/exile-leveling)
  (MIT) unmodified under `content/routes/exile-leveling/`, with license and
  attribution.
- A Rust parser (`route_dsl` module inside `content`) reads their DSL:
  `{enter|area_id}`, `{waypoint|…}`, `{kill|…}`, `{quest|…}`, `{portal|…}`,
  `{logout}`, `{dir|N}`, `#sub` hint lines, `#ifdef LEAGUE_START` blocks.
- Build-time compilation produces our content-pack JSON keyed by exile-leveling
  area IDs (e.g., `1_1_4_1`). League resync = copy new files + rebuild.
- League-start vs. normal variant: user-selectable flag; both compiled.

## 4. Layout Content (poelayouts.docx)

- A repo-committed extraction tool parses the docx once: all notes (all 10
  acts, included verbatim) and all ~189 images, mapped to zone/area IDs via a
  hand-maintained mapping table (docx zone headings → exile-leveling area IDs).
- Every note and image carries metadata: source, author, `audit_status`
  (`unaudited` → `verified` | `outdated` | `corrected`), first/last verified
  patch, free-text correction.
- Initial release ships everything as `unaudited`; the audit pass updates
  statuses without deleting history. Outdated notes render with a subtle stale
  marker; corrected text replaces display while preserving the original.
- **Images are committed to the public repo** with attribution to
  Engineering Eternity in the README, a `CREDITS.md`, and per-asset metadata.
  Decision recorded: proceed with credit; remove promptly if EE objects.
  (The Ravaged Square composite is the docx author's own.)
- Assets are PNG (as extracted). Layout notes/images layer onto route steps by
  area ID.

## 5. Overlay UX

**Bottom-docked filmstrip bar** (mockup option "Comfortable" — approved):

- Docked above the XP bar, centered between life/mana globes by default.
- Contents, left to right: zone name + act + layout count · all layout variant
  images side-by-side · text block (primary action with direction glyph, next
  step, build reminder + pending count).
- Direction glyph vocabulary from the PRD: `→` strong, `↗ ?` tendency,
  `⟳` wall-follow, `◇` find-landmark-first.
- **Click-through and no-activate during play.** No buttons in normal mode.
- **Resizable however the user likes:**
  - Setup mode (tray or settings) makes the bar interactive: drag to move,
    corner-drag to resize.
  - One scale factor resizes images and text together; bar width independently
    adjustable; image row wraps or scrolls never — variants shrink to fit.
  - Scale slider + "reset to default" in settings. Position/scale persisted
    per monitor resolution.
- **Hold-hotkey zoom:** a configurable hotkey temporarily enlarges the layout
  images over the game; release to restore. (Chosen hybrid of mockups B + C.)
- Town behavior: build reminders (gems to buy, links, gear) surface
  preferentially in town.
- Inspect mode (from tray/hotkey): larger variants, confidence rationale,
  pending task list, correction actions.

## 6. Public Repository

- `poe-campaign-copilot`, public on GitHub from day one.
- **MIT license for code.** `CREDITS.md` covers: Engineering Eternity (layout
  images), the poelayouts compilation author (notes), HeartofPhos
  exile-leveling (route data, MIT), Grinding Gear Games (Path of Exile;
  unaffiliated fan tool disclaimer).
- CI via GitHub Actions: `cargo test` + clippy on macOS and Windows runners;
  Tauri build check on both. Windows crate paths compile green before any
  manual Windows session.

## 7. Testing

- **Replay harness** (runs on macOS): golden Client.txt fixtures replay
  deterministically through tailer → parser → engines → composer. Includes a
  paced "fake play" mode that appends fixture lines to a temp Client.txt in
  real time so the live overlay can be watched end-to-end on the Mac.
- Unit/property tests: parser normalization, route transitions (all 10 acts
  compile and walk without deadlock), task-state transitions and contradiction
  recovery, DSL importer against vendored files, PoB fixtures (valid, malformed,
  huge, unsupported), content schema validation, composer snapshots.
- **Windows validation checklist** (manual, end of build): overlay flags over
  real PoE in windowed-fullscreen, click-through, no focus steal, DPI/multi-
  monitor, real Client.txt tailing during play, exclusive-fullscreen detected
  and warned as unsupported.

## 8. Build Order

1. Repo scaffold: Tauri 2 app, Rust workspace, CI, licenses/credits.
2. exile-leveling DSL parser + build-time route compilation.
3. docx extraction tool + zone mapping + content pack generation.
4. Log tailer + event parser + replay harness + fake-play simulator.
5. Route/task engines + composer.
6. Filmstrip overlay UI + setup mode (move/resize/scale) + hotkey zoom.
7. PoB import + settings/diagnostics UI.
8. Content audit pass (statuses on docx notes).
9. Windows validation session; fix flag/DPI issues; tag first release.

## Decision Log (this session)

| Decision | Choice |
|---|---|
| Scope | Acts 1–10, English clients only |
| Metrics | Dropped as gates; accuracy auditing retained |
| EE images | Include in public repo with attribution; remove on objection |
| Architecture | Single Tauri 2 app (supersedes PRD's native Win32 HUD) |
| Dev machine | Build on macOS; Windows machine for validation/testing |
| Route data | Vendor exile-leveling DSL files; parse + compile at build time |
| Overlay design | Bottom-docked comfortable filmstrip bar, above XP bar |
| Resizing | Full user control: drag, corner-resize, scale slider, persistence |
| Image zoom | Hold-hotkey temporary enlarge |
| Repo | Public GitHub, MIT code, CREDITS for third-party content |
