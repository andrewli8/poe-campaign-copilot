# Plan 5: Live Filmstrip Overlay UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A live, bottom-docked filmstrip overlay: the Tauri app tails Client.txt, runs the session→engines→composer pipeline, and renders the approved "comfortable filmstrip" bar with real layout images — watchable end-to-end on macOS via fake-play.

**Architecture:** A `pipeline` module inside `src-tauri` (pure, unit-tested, no Tauri types) turns log lines into `UiModel`s (composer's `OverlayModel` + data-URL-embedded images). `main.rs` wires: spawn the `input_log` tailer on a thread → feed lines through the pipeline → `emit("overlay-model", model)` to the webview. The overlay window is transparent/undecorated/always-on-top/click-through; a tray menu toggles Setup Mode (click-through off, resizable on) and Zoom (window grows, images enlarge). React renders a `FilmstripBar` component from the model; component tests run under vitest with plain props (no Tauri mocking). Log path comes from the `POE_COPILOT_LOG` env var for now (settings UI is Plan 6).

**Tech Stack:** Tauri 2 (tray-icon feature, window-state + global-shortcut plugins), base64 crate, React 18 + TypeScript, vitest + @testing-library/react.

## Global Constraints

- Composer stays pure — all Tauri-facing augmentation (data URLs, waiting states) lives in `src-tauri/src/pipeline.rs`.
- `UiModel` JSON uses serde's default snake_case field names; the TS interface mirrors them exactly.
- Click-through (`set_ignore_cursor_events(true)`) is enabled by default in play mode ON ALL PLATFORMS (Tauri v2 supports it on macOS — this makes the real behavior testable on the Mac); Setup Mode disables it and enables resizing/dragging.
- Image bytes are read from `content::layouts::layouts_dir()/assets` (repo-relative; packaging for distribution is a later plan — add a code comment saying so). Encode each image once (memoize per file).
- macOS transparency requires `"macOSPrivateApi": true` in tauri.conf.json.
- Capability permission identifiers may need small adjustments against the installed Tauri version's registry — adjusting identifiers to make the runtime accept them is expected mechanical work, not a deviation; record final ids in the report.
- The PRD's HOLD-to-zoom is implemented as a TOGGLE for MVP (global shortcut `Alt+Shift+Z` and tray item); record the deviation in the report — hold semantics need lower-level key hooks, deferred to Windows validation polish.
- Frontend tests render components with fixture props only — no Tauri API mocking. The event-subscription hook is isolated in its own file and excluded from unit-test scope.
- Edition 2024; commit format `<type>: <description>`; no AI attribution. Controller pushes after each task.

---

### Task 1: Pipeline module (Rust, pure, tested)

**Files:**
- Create: `src-tauri/src/pipeline.rs`
- Modify: `src-tauri/src/main.rs` (add `mod pipeline;`), `src-tauri/Cargo.toml` (deps)

**Interfaces:**
- Consumes: `content::{compile::{compile_route_pack, Variant}, game_data::load_vendored, layouts::{load_all_layouts, layouts_dir}}`, `session::{SessionTracker, SessionEvent}`, `event_parser::parse_line`, `route_engine::RouteEngine`, `task_engine::TaskEngine`, `composer::{compose, layouts_by_area, OverlayModel}`.
- Produces (in `pipeline`):
  - `#[derive(Debug, Clone, Serialize)] pub struct UiImage { pub file: String, pub stale: bool, pub data_url: String }`
  - `#[derive(Debug, Clone, Serialize)] pub struct UiModel { pub overlay: OverlayModel, pub images: Vec<UiImage>, pub waiting_for_log: bool }`
  - `pub struct Pipeline { ... }`:
    - `pub fn new() -> Result<Self, PipelineError>` — loads vendored areas, compiles the league-start route pack, loads layouts; `PipelineError` is a thiserror enum wrapping the underlying error types.
    - `pub fn on_line(&mut self, line: &str) -> Option<UiModel>` — parses, feeds the tracker; returns `Some(model)` only when at least one `SessionEvent::AreaEntered` was processed (recompose); LevelUp/Slain/etc. return `None` (no UI change yet).
    - `pub fn current_model(&mut self) -> UiModel` — compose now (used for initial state; before any area event, `waiting_for_log: true` with a default overlay).
- Data-URL encoding: `data:image/png;base64,<...>` via the `base64` crate, memoized in a `HashMap<String, String>`; missing image files produce an empty `data_url` (never an error).

- [ ] **Step 1: Add deps**

In `src-tauri/Cargo.toml` `[dependencies]` add:

```toml
base64 = "0.22"
thiserror = "2"
event_parser = { path = "../crates/event_parser" }
session = { path = "../crates/session" }
route_engine = { path = "../crates/route_engine" }
task_engine = { path = "../crates/task_engine" }
composer = { path = "../crates/composer" }
input_log = { path = "../crates/input_log" }
```

- [ ] **Step 2: Failing tests**

`src-tauri/src/pipeline.rs` — types + stubs + tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const GEN_STRAND: &str = r#"2026/07/21 19:00:01 1000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_1" with seed 101"#;
    const ENTER_STRAND: &str = "2026/07/21 19:00:03 3000 f22b6b26 [INFO Client 900] : You have entered The Twilight Strand.";
    const GEN_COAST: &str = r#"2026/07/21 19:04:00 240000 1186a0e0 [DEBUG Client 900] Generating level 2 area "1_1_2" with seed 8801"#;
    const ENTER_COAST: &str = "2026/07/21 19:04:02 242000 f22b6b26 [INFO Client 900] : You have entered The Coast.";
    const GEN_TOWN: &str = r#"2026/07/21 19:03:20 200000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_town" with seed 555"#;
    const ENTER_TOWN: &str = "2026/07/21 19:03:22 202000 f22b6b26 [INFO Client 900] : You have entered Lioneye's Watch.";

    #[test]
    fn initial_model_is_waiting() {
        let mut p = Pipeline::new().unwrap();
        let m = p.current_model();
        assert!(m.waiting_for_log);
    }

    #[test]
    fn area_entry_produces_model_with_images() {
        let mut p = Pipeline::new().unwrap();
        assert!(p.on_line(GEN_STRAND).is_none(), "generating alone changes nothing");
        let m = p.on_line(ENTER_STRAND).expect("entered -> model");
        assert!(!m.waiting_for_log);
        assert_eq!(m.overlay.zone_name, "The Twilight Strand");

        p.on_line(GEN_TOWN);
        p.on_line(ENTER_TOWN);
        p.on_line(GEN_COAST);
        let m = p.on_line(ENTER_COAST).unwrap();
        assert_eq!(m.overlay.zone_name, "The Coast");
        assert!(!m.images.is_empty(), "The Coast has layout images");
        assert!(m.images[0].data_url.starts_with("data:image/png;base64,"));
        assert!(m.images[0].data_url.len() > 100);
    }

    #[test]
    fn non_area_lines_produce_none() {
        let mut p = Pipeline::new().unwrap();
        assert!(p.on_line("garbage").is_none());
        assert!(p
            .on_line("2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2")
            .is_none());
    }

    #[test]
    fn data_urls_are_memoized() {
        let mut p = Pipeline::new().unwrap();
        p.on_line(GEN_COAST);
        let m1 = p.on_line(ENTER_COAST).unwrap();
        // Re-enter: same encoded strings (pointer equality is not observable;
        // assert cache hit via identical output and cache length).
        p.on_line(GEN_COAST);
        let m2 = p.on_line(ENTER_COAST).unwrap();
        assert_eq!(m1.images[0].data_url, m2.images[0].data_url);
        assert_eq!(p.encoded_cache_len(), m1.images.len());
    }
}
```

(Expose `pub(crate) fn encoded_cache_len(&self) -> usize` for the test.)

Run: `cargo test -p poe-copilot-app` — FAIL (stubs).

- [ ] **Step 3: Implement**

```rust
//! Pure pipeline: log lines -> UiModel. No Tauri types here.
//! NOTE: images load from the repo's content/layouts/assets at runtime —
//! fine for development; distribution packaging revisits this path.

use std::collections::HashMap;

use base64::Engine as _;
use composer::{compose, layouts_by_area, OverlayModel};
use content::compile::{compile_route_pack, Variant};
use content::game_data::{load_vendored, AreaMap};
use content::layouts::{layouts_dir, load_all_layouts, LayoutEntry};
use route_engine::RouteEngine;
use serde::Serialize;
use session::{SessionEvent, SessionTracker};
use task_engine::TaskEngine;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct UiImage {
    pub file: String,
    pub stale: bool,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiModel {
    pub overlay: OverlayModel,
    pub images: Vec<UiImage>,
    pub waiting_for_log: bool,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("game data: {0}")]
    GameData(#[from] content::game_data::GameDataError),
    #[error("route compile: {0}")]
    Compile(#[from] content::compile::CompileError),
    #[error("layouts: {0}")]
    Layouts(#[from] content::layouts::LayoutError),
}

pub struct Pipeline {
    tracker: SessionTracker,
    engine: RouteEngine,
    tasks: TaskEngine,
    layouts: std::collections::BTreeMap<String, LayoutEntry>,
    areas: AreaMap,
    seen_area_event: bool,
    encoded: HashMap<String, String>,
}

impl Pipeline {
    pub fn new() -> Result<Self, PipelineError> {
        let (areas, _) = load_vendored()?;
        let pack = compile_route_pack(Variant::LeagueStart)?;
        let engine = RouteEngine::from_pack(&pack, areas.clone());
        let layouts = layouts_by_area(load_all_layouts()?);
        Ok(Self {
            tracker: SessionTracker::new(areas.clone()),
            engine,
            tasks: TaskEngine::new(areas.clone()),
            layouts,
            areas,
            seen_area_event: false,
            encoded: HashMap::new(),
        })
    }

    pub fn on_line(&mut self, line: &str) -> Option<UiModel> {
        let raw = event_parser::parse_line(line);
        let mut area_changed = false;
        for ev in self.tracker.on_raw(&raw) {
            if let SessionEvent::AreaEntered { area_id, .. } = &ev {
                let advance = self.engine.on_area_entered(area_id);
                for &i in advance.newly_done.iter().chain(&advance.newly_skipped) {
                    let step = self.engine.steps()[i].clone();
                    let status = self.engine.statuses()[i];
                    self.tasks.on_step_passed(&step, status);
                }
                area_changed = true;
                self.seen_area_event = true;
            }
        }
        area_changed.then(|| self.current_model())
    }

    pub fn current_model(&mut self) -> UiModel {
        let overlay = compose(&self.engine, &self.tasks, &self.layouts, &self.areas);
        let images = overlay
            .layout_images
            .iter()
            .map(|iv| UiImage {
                file: iv.file.clone(),
                stale: iv.stale,
                data_url: self.data_url_for(&iv.file),
            })
            .collect();
        UiModel {
            overlay,
            images,
            waiting_for_log: !self.seen_area_event,
        }
    }

    fn data_url_for(&mut self, file: &str) -> String {
        if let Some(hit) = self.encoded.get(file) {
            return hit.clone();
        }
        let path = layouts_dir().join("assets").join(file);
        let url = match std::fs::read(&path) {
            Ok(bytes) => format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
            Err(_) => String::new(),
        };
        self.encoded.insert(file.to_string(), url.clone());
        url
    }

    #[cfg(test)]
    pub(crate) fn encoded_cache_len(&self) -> usize {
        self.encoded.len()
    }
}
```

Add `mod pipeline;` to `src-tauri/src/main.rs`. Adjust `TaskEngine::new(areas)` call to the real signature (it takes an AreaMap since Plan 4's fixes).

Run: `cargo test -p poe-copilot-app` — PASS. fmt + clippy + workspace green.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/ Cargo.lock
git commit -m "feat: log-to-overlay pipeline module in the app"
```

---

### Task 2: Tauri wiring — window, tray, tailer thread, commands

**Files:**
- Modify: `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `package.json` (plugin JS deps)

**Interfaces:**
- Produces:
  - Tauri commands: `get_model() -> UiModel`, `set_setup_mode(enabled: bool)`, `toggle_zoom() -> bool` (returns new zoom state).
  - Event `"overlay-model"` emitted with a `UiModel` payload on every zone change.
  - Event `"setup-mode"` emitted with a bool when setup mode toggles (tray or command).
  - Event `"zoom"` emitted with a bool when zoom toggles (tray, command, or `Alt+Shift+Z` global shortcut).
  - Window behavior: transparent, undecorated, always-on-top, skip-taskbar, 960×150 default, click-through ON at startup; Setup Mode → click-through OFF + resizable ON (reverse on exit). Zoom ON → window height 420; OFF → 150 (keep width/position).
  - Tray menu: "Setup Mode" (check item), "Zoom" (check item), "Quit".

- [ ] **Step 1: Config + deps**

`src-tauri/Cargo.toml`: change tauri dep to `tauri = { version = "2", features = ["tray-icon"] }`; add:

```toml
tauri-plugin-window-state = "2"
tauri-plugin-global-shortcut = "2"
```

`package.json` dependencies add: `"@tauri-apps/plugin-global-shortcut": "^2"` (window-state needs no JS side for our use). Run `npm install`.

`src-tauri/tauri.conf.json` — replace the `app` section's window and add macOSPrivateApi:

```json
  "app": {
    "macOSPrivateApi": true,
    "windows": [
      {
        "label": "main",
        "title": "PoE Campaign Copilot",
        "width": 960,
        "height": 150,
        "transparent": true,
        "decorations": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "resizable": false,
        "shadow": false
      }
    ],
    "security": { "csp": null }
  },
```

- [ ] **Step 2: main.rs wiring**

Rewrite `src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pipeline;

use std::sync::Mutex;

use pipeline::{Pipeline, UiModel};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, State};

struct AppState {
    pipeline: Mutex<Pipeline>,
    setup_mode: Mutex<bool>,
    zoom: Mutex<bool>,
}

#[tauri::command]
fn get_model(state: State<AppState>) -> UiModel {
    state.pipeline.lock().unwrap().current_model()
}

#[tauri::command]
fn set_setup_mode(app: tauri::AppHandle, enabled: bool) {
    apply_setup_mode(&app, enabled);
}

#[tauri::command]
fn toggle_zoom(app: tauri::AppHandle) -> bool {
    toggle_zoom_impl(&app)
}

fn apply_setup_mode(app: &tauri::AppHandle, enabled: bool) {
    let state: State<AppState> = app.state();
    *state.setup_mode.lock().unwrap() = enabled;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_ignore_cursor_events(!enabled);
        let _ = win.set_resizable(enabled);
    }
    let _ = app.emit("setup-mode", enabled);
}

fn toggle_zoom_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let mut zoom = state.zoom.lock().unwrap();
    *zoom = !*zoom;
    let new_zoom = *zoom;
    drop(zoom);
    if let Some(win) = app.get_webview_window("main") {
        if let Ok(size) = win.outer_size() {
            let height = if new_zoom { 420 } else { 150 };
            let _ = win.set_size(tauri::PhysicalSize::new(size.width, height));
        }
    }
    let _ = app.emit("zoom", new_zoom);
    new_zoom
}

fn main() {
    let pipeline = Pipeline::new().expect("content data must load");

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+shift+z"])
                .expect("valid shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        toggle_zoom_impl(app);
                    }
                })
                .build(),
        )
        .manage(AppState {
            pipeline: Mutex::new(pipeline),
            setup_mode: Mutex::new(false),
            zoom: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![get_model, set_setup_mode, toggle_zoom])
        .setup(|app| {
            // Tray menu
            let setup_item =
                CheckMenuItem::with_id(app, "setup", "Setup Mode", true, false, None::<&str>)?;
            let zoom_item =
                CheckMenuItem::with_id(app, "zoom", "Zoom", true, false, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&setup_item, &zoom_item, &quit_item])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "setup" => {
                        let state: State<AppState> = app.state();
                        let enabled = !*state.setup_mode.lock().unwrap();
                        apply_setup_mode(app, enabled);
                    }
                    "zoom" => {
                        toggle_zoom_impl(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Click-through by default (play mode).
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_ignore_cursor_events(true);
            }

            // Tailer thread: POE_COPILOT_LOG -> pipeline -> emit.
            if let Ok(log_path) = std::env::var("POE_COPILOT_LOG") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let poller = match input_log::FilePoller::new(log_path.into(), false) {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("tailer: cannot open log: {e}");
                            return;
                        }
                    };
                    let (tx, rx) = std::sync::mpsc::channel();
                    let _tailer = input_log::spawn_tailer(
                        poller,
                        std::time::Duration::from_millis(250),
                        tx,
                    );
                    for line in rx {
                        let state: State<AppState> = handle.state();
                        let model = state.pipeline.lock().unwrap().on_line(&line);
                        if let Some(model) = model {
                            let _ = handle.emit("overlay-model", &model);
                        }
                    }
                });
            } else {
                eprintln!("POE_COPILOT_LOG not set — overlay will wait (settings UI comes later)");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Capabilities**

`src-tauri/capabilities/default.json` — replace permissions with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-set-ignore-cursor-events",
    "core:window:allow-set-size",
    "core:window:allow-set-resizable",
    "core:window:allow-start-dragging",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister"
  ]
}
```

(If the runtime rejects an identifier, consult `src-tauri/gen/schemas` after a build for the exact registry names and adjust — record final ids in the report.)

- [ ] **Step 4: Verify**

`cargo check -p poe-copilot-app` clean; `cargo test --workspace` green; `npm run build` clean. Manual: `POE_COPILOT_LOG=/tmp/fake-client.txt npm run tauri dev` — a transparent, undecorated bar-shaped window appears (content still placeholder React); tray icon appears with the three items; toggling Setup Mode makes the window clickable/resizable. Note: full visual verification lands in Task 4.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/ package.json package-lock.json Cargo.lock
git commit -m "feat: overlay window, tray, tailer wiring, and app commands"
```

---

### Task 3: React filmstrip UI + component tests

**Files:**
- Create: `src/types.ts`, `src/FilmstripBar.tsx`, `src/FilmstripBar.css`, `src/useOverlay.ts`, `src/FilmstripBar.test.tsx`, `vitest.config.ts`, `src/test-setup.ts`
- Modify: `src/App.tsx`, `src/main.tsx` (import CSS), `package.json` (test deps + script), `tsconfig.json` (vitest types if needed)

**Interfaces:**
- `src/types.ts` mirrors the Rust payloads exactly (snake_case):

```ts
export interface NoteView { text: string; stale: boolean }
export interface UiImage { file: string; stale: boolean; data_url: string }
export interface OverlayModel {
  zone_name: string;
  area_id: string;
  act: number;
  off_route_zone: string | null;
  layout_images: { file: string; stale: boolean }[];
  layout_notes: NoteView[];
  steps_in_zone: string[];
  sub_hints: string[];
  primary: string;
  next_zone: string | null;
  pending_count: number;
  town_reminders: string[];
  is_town: boolean;
  route_complete: boolean;
}
export interface UiModel {
  overlay: OverlayModel;
  images: UiImage[];
  waiting_for_log: boolean;
}
```

- `FilmstripBar({ model, zoom, setupMode }: { model: UiModel; zoom: boolean; setupMode: boolean })` — pure presentational component:
  - waiting state: a small centered pill "Waiting for Client.txt…"
  - normal: header row (zone name, `ACT {act}`, layout count, pending badge `○ N pending` when > 0, off-route banner "In {off_route_zone} — off route" when set, `TOWN` chip when is_town)
  - image row: `img` per UiImage with `src=data_url`, class `stale` when stale (rendered with reduced opacity + "outdated" badge); zoom doubles the image height via the `zoom` CSS class on the root
  - text block: `primary` bold; remaining `steps_in_zone` as a list; `sub_hints` as smaller hint lines; "Next: {next_zone}"; `town_reminders` list when present
  - route_complete: celebratory "Campaign complete" bar
  - setupMode: shows a thin border + drag hint ("drag to move · resize edges · toggle via tray")
- `useOverlay()` hook (in its own file, NOT unit-tested): initial `invoke("get_model")`, subscribes to `overlay-model`, `setup-mode`, `zoom` events; returns `{ model, zoom, setupMode }`.
- Styling: dark translucent bar per the approved mockup — background `rgba(10,10,12,0.82)`, 1px `rgba(255,255,255,0.12)` border, 6px radius, gold `#c8a95a` zone label (11px uppercase, letter-spacing), white 13px primary, muted `#8a94a8` next-line, green `#6fbf73` reminders, image thumbs 118px wide (border-radius 3px), body background fully transparent.

- [ ] **Step 1: Test deps + config**

`package.json` devDependencies add: `"vitest": "^3"`, `"@testing-library/react": "^16"`, `"@testing-library/jest-dom": "^6"`, `"jsdom": "^25"`; scripts add `"test": "vitest run"`. `npm install`.

`vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.tsx"],
  },
});
```

`src/test-setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 2: Failing component tests**

`src/FilmstripBar.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FilmstripBar } from "./FilmstripBar";
import type { UiModel } from "./types";

const PIXEL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNsb2j4DwAFKAJ003oL8QAAAABJRU5ErkJggg==";

function model(overrides: Partial<UiModel["overlay"]> = {}, extra: Partial<UiModel> = {}): UiModel {
  return {
    overlay: {
      zone_name: "The Coast",
      area_id: "1_1_2",
      act: 1,
      off_route_zone: null,
      layout_images: [{ file: "a.png", stale: false }],
      layout_notes: [{ text: "Follow the right wall.", stale: false }],
      steps_in_zone: ["Get waypoint", "➞ The Mud Flats"],
      sub_hints: ["Go ↗"],
      primary: "Get waypoint",
      next_zone: "The Mud Flats",
      pending_count: 2,
      town_reminders: [],
      is_town: false,
      route_complete: false,
      ...overrides,
    },
    images: [{ file: "a.png", stale: false, data_url: PIXEL }],
    waiting_for_log: false,
    ...extra,
  };
}

describe("FilmstripBar", () => {
  it("renders zone, act, primary, next and pending badge", () => {
    render(<FilmstripBar model={model()} zoom={false} setupMode={false} />);
    expect(screen.getByText("The Coast")).toBeInTheDocument();
    expect(screen.getByText(/act 1/i)).toBeInTheDocument();
    expect(screen.getByText("Get waypoint")).toBeInTheDocument();
    expect(screen.getByText(/next: the mud flats/i)).toBeInTheDocument();
    expect(screen.getByText(/2 pending/i)).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", PIXEL);
  });

  it("shows waiting state before any log data", () => {
    render(
      <FilmstripBar
        model={model({}, { waiting_for_log: true })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/waiting for client\.txt/i)).toBeInTheDocument();
    expect(screen.queryByText("The Coast")).not.toBeInTheDocument();
  });

  it("shows off-route banner and town reminders in town", () => {
    render(
      <FilmstripBar
        model={model({
          off_route_zone: "Lioneye's Watch",
          is_town: true,
          town_reminders: ["Claim quest reward: Quicksilver Flask"],
        })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/off route/i)).toBeInTheDocument();
    expect(screen.getByText(/quicksilver flask/i)).toBeInTheDocument();
  });

  it("marks stale images and notes", () => {
    const m = model({ layout_notes: [{ text: "Old info.", stale: true }] });
    m.images = [{ file: "a.png", stale: true, data_url: PIXEL }];
    render(<FilmstripBar model={m} zoom={false} setupMode={false} />);
    expect(screen.getByText(/outdated/i)).toBeInTheDocument();
    expect(screen.getByText("Old info.")).toHaveClass("stale");
  });

  it("renders campaign complete state", () => {
    render(
      <FilmstripBar
        model={model({ route_complete: true, zone_name: "Campaign complete" })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/campaign complete/i)).toBeInTheDocument();
  });

  it("applies zoom and setup-mode classes", () => {
    const { container } = render(
      <FilmstripBar model={model()} zoom={true} setupMode={true} />,
    );
    expect(container.firstChild).toHaveClass("zoom");
    expect(screen.getByText(/drag to move/i)).toBeInTheDocument();
  });
});
```

Run: `npm test` — FAIL (component missing).

- [ ] **Step 3: Implement component, hook, wiring**

`src/FilmstripBar.tsx` — implement per the Interfaces spec (all elements/classes the tests query: root gets `filmstrip` + conditional `zoom`; stale notes get class `stale`; stale images get an "outdated" badge). `src/FilmstripBar.css` per the styling spec. `src/useOverlay.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { UiModel } from "./types";

export function useOverlay() {
  const [model, setModel] = useState<UiModel | null>(null);
  const [zoom, setZoom] = useState(false);
  const [setupMode, setSetupMode] = useState(false);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    invoke<UiModel>("get_model").then((m) => {
      if (!disposed) setModel(m);
    });
    listen<UiModel>("overlay-model", (e) => setModel(e.payload)).then((u) => unlisteners.push(u));
    listen<boolean>("zoom", (e) => setZoom(e.payload)).then((u) => unlisteners.push(u));
    listen<boolean>("setup-mode", (e) => setSetupMode(e.payload)).then((u) => unlisteners.push(u));
    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, []);

  return { model, zoom, setupMode };
}
```

`src/App.tsx`:

```tsx
import { FilmstripBar } from "./FilmstripBar";
import { useOverlay } from "./useOverlay";

export default function App() {
  const { model, zoom, setupMode } = useOverlay();
  if (!model) return null;
  return <FilmstripBar model={model} zoom={zoom} setupMode={setupMode} />;
}
```

Set the page background transparent (in `FilmstripBar.css` or an `index.css`): `html, body, #root { background: transparent; margin: 0; }`.

Run: `npm test` — all PASS. `npm run build` clean. `cargo test --workspace` still green.

- [ ] **Step 4: Commit**

```bash
git add src/ vitest.config.ts package.json package-lock.json
git commit -m "feat: filmstrip overlay UI with component tests"
```

---

### Task 4: Live macOS validation + screenshot + docs

**Files:**
- Modify: `README.md`
- Create (scratch, not committed): a screenshot at `/private/tmp/claude-501/-Users-andrew-Desktop-projects/a0b6925a-7d81-4914-8a6b-5455239e91af/scratchpad/overlay-live.png`

- [ ] **Step 1: Live run**

```bash
rm -f /tmp/fake-client.txt && touch /tmp/fake-client.txt
POE_COPILOT_LOG=/tmp/fake-client.txt npm run tauri dev   # background/long-running
# wait for compile + window
cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 800
```

Expected: the bar appears; as fake-play writes, the bar transitions Waiting → Twilight Strand → Lioneye's Watch → The Coast (with layout images) → off-route banner on the early town revisit → Mud Flats. 

- [ ] **Step 2: Screenshot evidence**

While the overlay shows The Coast (or any state with images), capture: `screencapture -x <scratchpad>/overlay-live.png` (full screen is fine). If macOS screen-recording permission blocks capture, note it in the report and describe the observed states textually instead.

Then kill the dev process and clean up `/tmp/fake-client.txt`.

- [ ] **Step 3: README + commit**

Update the README "Try it" flow (Development section) with the two-terminal fake-play + `POE_COPILOT_LOG` instructions. Full gate: `cargo test --workspace`, fmt, clippy, `npm test`, `npm run build`.

```bash
git add README.md
git commit -m "docs: live overlay demo instructions for macOS"
```

---

## Verification (end of plan)

- [ ] Full gate green (Rust + vitest).
- [ ] Live demo observed on macOS with screenshot (or documented permission block).
- [ ] CI green after push (CI runs cargo + `npm run build`; add `npm test` to the CI app job in this plan's final task if trivial — optional).

## Self-Review Notes

- Spec coverage: §5 overlay UX — filmstrip bar ✅, click-through/no-activate ✅ (testable on mac), setup mode move/resize ✅ (window-state persistence + resizable toggle; scale slider deferred: window resize + zoom cover MVP scaling — record as deviation), hold-zoom → toggle-zoom deviation recorded, town reminders ✅, stale markers ✅. FR-10 partial (DPI/multi-monitor validation is the Windows session's job).
- Contract fidelity: types.ts mirrors OverlayModel/UiModel field-for-field (snake_case); UiImage embeds data URLs so the webview needs no filesystem access.
- Known deferrals: log-path settings UI (Plan 6), PoB build reminders (Plan 6), scale slider polish, hold-to-zoom, player-level display (needs pinning).
