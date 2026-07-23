#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod diagnostics;
mod hotkeys;
mod journal;
mod pipeline;
mod run_timer;

use std::sync::Mutex;

use config::AppConfig;
use content::compile::Variant;
use input_log::TailerHandle;
use pipeline::{Pipeline, UiModel};
use pob_import::LevelingBuildPlan;
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_window_state::StateFlags;

struct AppState {
    pipeline: Mutex<Pipeline>,
    setup_mode: Mutex<bool>,
    zoom: Mutex<bool>,
    compact: Mutex<bool>,
    hidden: Mutex<bool>,
    setup_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    zoom_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    compact_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    hide_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// The currently running Client.txt tailer, if any (`None` while
    /// waiting for a log path to be configured). Swapped out by
    /// `apply_settings` whenever the configured path changes.
    tailer: Mutex<Option<TailerHandle>>,
    /// Pausable campaign run timer; every transition is persisted to
    /// run_timer.json and broadcast on the "run-timer" event.
    run_timer: Mutex<run_timer::RunTimerState>,
}

#[derive(Debug, Serialize)]
struct PobSummary {
    class_name: String,
    ascend_name: Option<String>,
    milestone_count: usize,
    reliability: String,
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

#[tauri::command]
fn toggle_compact(app: tauri::AppHandle) -> bool {
    toggle_compact_impl(&app)
}

#[tauri::command]
fn toggle_hide(app: tauri::AppHandle) -> bool {
    toggle_hide_impl(&app)
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> AppConfig {
    config::load(&app)
}

#[tauri::command]
fn get_run_timer(state: State<AppState>) -> run_timer::RunTimerState {
    *state.run_timer.lock().unwrap()
}

/// Blocks the invoking thread until the user picks a file or closes the
/// dialog. This is safe to call directly, with no async wrapper or manual
/// off-thread dispatch: Tauri v2's IPC layer never runs a command handler
/// on the main/event-loop thread, so blocking here doesn't freeze the UI —
/// this is exactly the usage `tauri-plugin-dialog` documents for
/// `FileDialogBuilder::blocking_pick_file`.
#[tauri::command]
fn pick_log_file(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Select Client.txt")
        .blocking_pick_file()
        .and_then(|fp| fp.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Decodes and parses a Path of Building share code/XML into a preview
/// summary WITHOUT saving it — the settings UI uses this to validate a
/// pasted code before the user commits to `apply_settings`.
#[tauri::command]
fn import_pob(code: String) -> Result<PobSummary, String> {
    let plan = parse_pob_code(&code)?;
    Ok(PobSummary {
        class_name: plan.class_name,
        ascend_name: plan.ascend_name,
        milestone_count: plan.milestones.len(),
        reliability: reliability_str(plan.reliability),
    })
}

#[tauri::command]
fn apply_settings(app: tauri::AppHandle, cfg: AppConfig) -> Result<(), String> {
    let old_cfg = config::load(&app);

    // Clamp the opacity up front so the no-op guard, the persisted file,
    // and the emitted event all agree on the value that actually applies.
    let cfg = AppConfig {
        overlay_opacity: config::clamp_opacity(cfg.overlay_opacity),
        ..cfg
    };

    // (0) No-op guard: if the incoming config is byte-for-byte identical to
    // what's already persisted, skip the rebuild entirely and report
    // success. Settings can be reopened and Saved without changing
    // anything (or Save can be double-clicked), and rebuilding the
    // pipeline/tailer on every Save — even a no-op one — would tear down
    // and recreate route-engine/task-engine state mid-run, resetting route
    // progress and un-pinning the player's level for no reason.
    if config::configs_equal(&old_cfg, &cfg) {
        return Ok(());
    }

    // (a) Validate first: nothing below should mutate shared state or the
    // config file on disk if the submitted settings don't even parse.
    let variant = map_variant(&cfg.variant)?;
    hotkeys::validate(&cfg.hotkeys)?;

    // Pipeline/tailer rebuild only when a field that FEEDS it changed —
    // an opacity/hotkey-only Save must not reset in-progress route state.
    let pipeline_changed = !config::pipeline_configs_equal(&old_cfg, &cfg);
    if pipeline_changed {
        let build = match cfg.pob_code.as_deref().map(str::trim) {
            Some(code) if !code.is_empty() => Some(parse_pob_code(code)?),
            _ => None,
        };
        if let Some(path) = &cfg.client_log_path
            && !is_regular_file(path)
        {
            // Same error as "doesn't exist" — a path that exists but isn't a
            // regular file (a directory, a FIFO, a device node, ...) is just as
            // unusable as a missing one: pointing the tailer's poller at a
            // directory or blocking special file would hang or misbehave the
            // tailer thread rather than fail cleanly here.
            return Err(format!("log file not found: {path}"));
        }

        // (b) Build the new Pipeline before touching any shared state or the
        // config file. If content data fails to load here, `apply_settings`
        // returns before the tailer is touched or the config is saved, so a
        // bad settings submission can't leave the live app (or the next
        // launch, which re-`Pipeline::new`s from the saved config and
        // `.expect()`s success) in a broken state.
        let new_pipeline = Pipeline::new(variant, build).map_err(|e| e.to_string())?;

        // (c) Stop the OLD tailer's producer thread BEFORE swapping the
        // pipeline into state.pipeline. This ordering matters: the tailer's
        // consumer thread (the `for line in rx` loop spawned in `spawn_tail`)
        // re-fetches `state.pipeline` fresh on every line rather than holding
        // a reference to a specific Pipeline instance, so if the pipeline swap
        // (d) happened first, any Client.txt lines still arriving from the OLD
        // log file would get fed into the NEW pipeline — wrong route/variant/
        // build context — for as long as the old tailer kept running (up to
        // its 250ms poll interval, or indefinitely on an idle log). Stopping
        // first closes that window: `TailerHandle::stop()` blocks until the
        // producer thread has exited and dropped its `Sender`, which in turn
        // ends the consumer thread's `for line in rx` loop once it drains
        // whatever was already buffered. See the consumer-thread trace note
        // below `spawn_tail` for the (much narrower, and accepted) residual
        // race this doesn't fully close.
        let old_tailer = {
            let state: State<AppState> = app.state();
            state.tailer.lock().unwrap().take()
        };
        if let Some(tailer) = old_tailer {
            tailer.stop();
        }

        // (d) Now it's safe to swap the pipeline: the old producer is gone.
        {
            let state: State<AppState> = app.state();
            *state.pipeline.lock().unwrap() = new_pipeline;
        }

        // (d2) The new pipeline starts with fresh route/session state, so the
        // session journal from the old configuration must go with it — leaving
        // it behind would resurrect the discarded progress (possibly from a
        // different log file, route variant, or build) on the next launch.
        // Only runs on a pipeline-feeding change: an opacity/hotkey-only Save
        // keeps both the live progress and its journal. Non-fatal on failure:
        // the Save itself still succeeded.
        if let Some(journal_path) = journal::journal_path(&app)
            && let Err(e) = journal::clear(&journal_path)
        {
            diagnostics::diag(&format!("journal: {e}"));
        }

        // (d3) A new log path/route/build is a new run: reset the run
        // timer along with the session journal. Same non-fatal contract.
        reset_run_timer_impl(&app);

        // (e) Spawn the new tailer at the configured path. A failure here
        // (e.g. the configured path was removed between validation above and
        // now) propagates as an error rather than being swallowed: the caller
        // ordering below means config::save (f) never runs in that case, so a
        // Save that can't actually start tailing doesn't get persisted as if
        // it had succeeded.
        if let Some(path) = cfg.client_log_path.clone() {
            spawn_tail(&app, path, true, journal::journal_path(&app))?;
        }
    }

    // (e2) Re-register global shortcuts if the bindings changed. Old ones
    // are unregistered first; if registering the NEW set fails (a combo
    // grabbed by the OS or another app — something `validate` can't see),
    // the old bindings are restored and the error propagates, so config
    // save below never records bindings that aren't actually active.
    if old_cfg.hotkeys != cfg.hotkeys {
        hotkeys::unregister_all(&app, &old_cfg.hotkeys);
        if let Err(e) = hotkeys::register_all(&app, &cfg.hotkeys, dispatch_hotkey) {
            if let Err(revert_err) = hotkeys::register_all(&app, &old_cfg.hotkeys, dispatch_hotkey)
            {
                diagnostics::diag(&format!(
                    "hotkeys: failed to restore previous bindings: {revert_err}"
                ));
            }
            return Err(e);
        }
    }

    // (f) Persist only after the rebuild above fully succeeded.
    config::save(&app, &cfg)?;

    // (f2) Push the (possibly unchanged) persisted opacity to the overlay
    // window — this also snaps any un-saved slider preview value back to
    // what was actually saved.
    if let Err(e) = app.emit("overlay-opacity", cfg.overlay_opacity) {
        diagnostics::diag(&format!(
            "apply_settings: failed to emit overlay-opacity: {e}"
        ));
    }

    // (f3) Push the persisted run-timer visibility to the overlay window.
    if let Err(e) = app.emit("show-run-timer", cfg.show_run_timer) {
        diagnostics::diag(&format!(
            "apply_settings: failed to emit show-run-timer: {e}"
        ));
    }

    // (g) Emit a fresh model with no lock held.
    if pipeline_changed {
        let model = {
            let state: State<AppState> = app.state();
            state.pipeline.lock().unwrap().current_model()
        };
        let _ = app.emit("overlay-model", &model);
    }
    Ok(())
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    open_settings_window(&app);
}

/// Live opacity preview from the settings slider: clamps and broadcasts
/// the value (the overlay window listens for "overlay-opacity") WITHOUT
/// persisting it — persistence happens on Save via `apply_settings`.
/// Returns the clamped value so the caller can reflect what was applied.
#[tauri::command]
fn set_overlay_opacity(app: tauri::AppHandle, opacity: f64) -> f64 {
    let clamped = config::clamp_opacity(opacity);
    if let Err(e) = app.emit("overlay-opacity", clamped) {
        diagnostics::diag(&format!("set_overlay_opacity: failed to emit: {e}"));
    }
    clamped
}

/// Routes a fired global hotkey to its action. Lives here (not in the
/// hotkeys module) because the toggle implementations are main.rs-private.
fn dispatch_hotkey(app: &tauri::AppHandle, action: hotkeys::HotkeyAction) {
    match action {
        hotkeys::HotkeyAction::Zoom => {
            toggle_zoom_impl(app);
        }
        hotkeys::HotkeyAction::Compact => {
            toggle_compact_impl(app);
        }
        hotkeys::HotkeyAction::Hide => {
            toggle_hide_impl(app);
        }
        hotkeys::HotkeyAction::Setup => {
            let enabled = {
                let state: State<AppState> = app.state();
                let current = *state.setup_mode.lock().unwrap();
                !current
            };
            apply_setup_mode(app, enabled);
        }
        hotkeys::HotkeyAction::Settings => {
            open_settings_window(app);
        }
        hotkeys::HotkeyAction::Timer => {
            toggle_run_timer_impl(app);
        }
    }
}

fn open_settings_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        if let Err(e) = win.set_focus() {
            diagnostics::diag(&format!(
                "open_settings: failed to focus existing window: {e}"
            ));
        }
        return;
    }
    // Build the window on the event loop rather than inline.
    //
    // When this is reached from the settings hotkey (alt+shift+o), we are
    // executing inside the macOS Carbon hot-key callback that
    // `tauri-plugin-global-shortcut` installs via
    // `InstallEventHandler(GetApplicationEventTarget(), …)`. That callback
    // fires re-entrantly, from within the main run loop's own event dispatch,
    // and creating an NSWindow/WKWebView from there crashes the app. The tray
    // menu never crashed because its "settings" item is dispatched by tao's
    // normal event loop, not re-entrantly. `run_on_main_thread` hands the
    // build back to that same event loop, so every entry point (hotkey, tray
    // menu, setup) creates the window in the one context that is known-good.
    let app_for_build = app.clone();
    let schedule = app.run_on_main_thread(move || {
        let result = tauri::WebviewWindowBuilder::new(
            &app_for_build,
            "settings",
            tauri::WebviewUrl::App("index.html?window=settings".into()),
        )
        .title("Settings")
        .inner_size(560.0, 680.0)
        .decorations(true)
        .resizable(true)
        .focused(true)
        .build();
        if let Err(e) = result {
            diagnostics::diag(&format!(
                "open_settings: failed to create settings window: {e}"
            ));
        }
    });
    if let Err(e) = schedule {
        diagnostics::diag(&format!(
            "open_settings: failed to schedule window creation: {e}"
        ));
    }
}

/// True only for a path that exists AND is a regular file — rejects a
/// directory, a FIFO/named pipe, a device node, a socket, etc. A bare
/// `Path::exists()` check (the previous behavior) is true for all of those
/// too, which would let `apply_settings` point the Client.txt tailer's
/// poller at something that isn't a normal append-only text file: a
/// directory fails to open sensibly, and a FIFO can block the tailer
/// thread indefinitely waiting for a writer.
fn is_regular_file(path: &str) -> bool {
    std::path::Path::new(path)
        .metadata()
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Maps the config's persisted variant string to the compiled route pack
/// selector. Anything other than the two known variants is a validation
/// error (surfaced to the settings UI by `apply_settings`); callers that
/// can't fail loudly (startup) fall back to `Variant::LeagueStart` instead.
fn map_variant(s: &str) -> Result<Variant, String> {
    match s {
        "league-start" => Ok(Variant::LeagueStart),
        "standard" => Ok(Variant::Standard),
        other => Err(format!("unknown route variant: {other}")),
    }
}

fn reliability_str(r: pob_import::Reliability) -> String {
    match r {
        pob_import::Reliability::Explicit => "explicit",
        pob_import::Reliability::Structured => "structured",
        pob_import::Reliability::Inferred => "inferred",
        pob_import::Reliability::Unsupported => "unsupported",
    }
    .to_string()
}

fn parse_pob_code(code: &str) -> Result<LevelingBuildPlan, String> {
    let gems = content::game_data::load_vendored_gems().map_err(|e| e.to_string())?;
    let (areas, quests) = content::game_data::load_vendored().map_err(|e| e.to_string())?;
    let xml = pob_import::decode_share_code(code).map_err(|e| e.to_string())?;
    pob_import::parse_build(&xml, &gems, &quests, &areas).map_err(|e| e.to_string())
}

fn apply_setup_mode(app: &tauri::AppHandle, enabled: bool) {
    let state: State<AppState> = app.state();
    *state.setup_mode.lock().unwrap() = enabled;
    if let Some(win) = app.get_webview_window("main") {
        if let Err(e) = win.set_ignore_cursor_events(!enabled) {
            diagnostics::diag(&format!(
                "apply_setup_mode: failed to set ignore-cursor-events: {e}"
            ));
        }
        let _ = win.set_resizable(enabled);
    }
    if let Some(item) = state.setup_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(enabled);
    }
    let _ = app.emit("setup-mode", enabled);
}

/// Clamp range for the content-driven overlay height, in logical pixels.
/// Duplicated in `src/overlayHeight.ts` and as `.filmstrip { max-height }`
/// in `src/FilmstripBar.css` — keep all three in step.
const MIN_OVERLAY_HEIGHT: f64 = 36.0;
const MAX_OVERLAY_HEIGHT: f64 = 600.0;

/// True only for a finite height inside `[MIN_OVERLAY_HEIGHT,
/// MAX_OVERLAY_HEIGHT]`. `set_overlay_height` is an IPC boundary, so a
/// NaN/infinite/out-of-range value from the webview is rejected rather
/// than passed to `set_size`.
fn overlay_height_in_range(height: f64) -> bool {
    height.is_finite() && (MIN_OVERLAY_HEIGHT..=MAX_OVERLAY_HEIGHT).contains(&height)
}

/// Resize the overlay to a content-measured height. Width is read back
/// from the current window and left unchanged, so only the bottom edge
/// moves (top-pinned growth). Out-of-range heights are rejected; a missing
/// window (shutdown race) is a no-op success.
#[tauri::command]
fn set_overlay_height(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    if !overlay_height_in_range(height) {
        let msg = format!("set_overlay_height: rejected out-of-range height {height}");
        diagnostics::diag(&msg);
        return Err(msg);
    }
    let Some(win) = app.get_webview_window("main") else {
        return Ok(());
    };
    if let (Ok(scale), Ok(size)) = (win.scale_factor(), win.outer_size()) {
        let logical = size.to_logical::<f64>(scale);
        if let Err(e) = win.set_size(tauri::LogicalSize::new(logical.width, height)) {
            diagnostics::diag(&format!("set_overlay_height: failed to resize window: {e}"));
            return Err(e.to_string());
        }
    }
    Ok(())
}

fn toggle_zoom_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let new_zoom = {
        let mut zoom = state.zoom.lock().unwrap();
        *zoom = !*zoom;
        *zoom
    };
    if let Some(item) = state.zoom_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_zoom);
    }
    // The window is not resized here: flipping `zoom` re-renders the
    // overlay (via the "zoom" event below), its content height changes,
    // and the frontend's ResizeObserver drives `set_overlay_height`.
    let _ = app.emit("zoom", new_zoom);
    new_zoom
}

/// Flips `AppState.compact` and re-renders the overlay. The window height
/// is not set here — the "compact" event re-renders the bar, and the
/// frontend's ResizeObserver resizes the window to the new content
/// (see `set_overlay_height`).
fn toggle_compact_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let new_compact = {
        let mut compact = state.compact.lock().unwrap();
        *compact = !*compact;
        *compact
    };
    if let Some(item) = state.compact_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_compact);
    }
    let _ = app.emit("compact", new_compact);
    new_compact
}

/// Flips `AppState.hidden` and shows/hides the "main" overlay window to
/// match, mirroring `toggle_zoom_impl`: the `hidden` guard is held across the
/// show/hide call so a second `toggle_hide_impl` (rapid tray clicks or the
/// global shortcut firing twice) can't interleave with it and leave the
/// window's actual visibility out of sync with the flag it reflects.
fn toggle_hide_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let mut hidden = state.hidden.lock().unwrap();
    *hidden = !*hidden;
    let new_hidden = *hidden;

    if let Some(win) = app.get_webview_window("main") {
        if new_hidden {
            if let Err(e) = win.hide() {
                diagnostics::diag(&format!("toggle_hide: failed to hide window: {e}"));
            }
        } else {
            if let Err(e) = win.show() {
                diagnostics::diag(&format!("toggle_hide: failed to show window: {e}"));
            }
            // The overlay is transparent/always-on-top/click-through by
            // design. `always_on_top`/`transparent` are static window
            // attributes from tauri.conf.json that `show()` is expected to
            // preserve on its own, but click-through is runtime state
            // (`AppState.setup_mode`) applied via `set_ignore_cursor_events`
            // — re-assert it defensively here in case a platform's `show()`
            // doesn't fully preserve window attributes set before a hide.
            let setup_mode = *state.setup_mode.lock().unwrap();
            if let Err(e) = win.set_ignore_cursor_events(!setup_mode) {
                diagnostics::diag(&format!(
                    "toggle_hide: failed to restore ignore-cursor-events: {e}"
                ));
            }
        }
    }
    drop(hidden);

    if let Some(item) = state.hide_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_hidden);
    }
    let _ = app.emit("hidden", new_hidden);
    new_hidden
}

/// Applies a pure run-timer transition under the state lock, then (only
/// if the state actually changed) persists and broadcasts it. The lock is
/// held across store() so two rapid transitions can't interleave their
/// writes and leave run_timer.json stale.
fn apply_run_timer_transition(
    app: &tauri::AppHandle,
    transition: impl Fn(run_timer::RunTimerState, u64) -> run_timer::RunTimerState,
) {
    let state: State<AppState> = app.state();
    let mut timer = state.run_timer.lock().unwrap();
    let new_state = transition(*timer, run_timer::now_ms());
    if new_state == *timer {
        return;
    }
    *timer = new_state;
    if let Some(path) = run_timer::path(app) {
        run_timer::store(&path, &new_state);
    }
    drop(timer);
    let _ = app.emit("run-timer", new_state);
}

fn toggle_run_timer_impl(app: &tauri::AppHandle) {
    apply_run_timer_transition(app, run_timer::toggle);
}

/// Zone-entry auto-start: only fires from the pristine never-started
/// state, so a manually paused timer stays paused across zone changes.
fn auto_start_run_timer(app: &tauri::AppHandle) {
    apply_run_timer_transition(app, |state, now| {
        if run_timer::is_never_started(&state) {
            run_timer::start(state, now)
        } else {
            state
        }
    });
}

/// Tray reset: back to never-started; the next zone entry (or the hotkey)
/// starts a new run. clear() (not store(default)) so a fresh install and
/// a reset look identical on disk.
fn reset_run_timer_impl(app: &tauri::AppHandle) {
    let state: State<AppState> = app.state();
    let mut timer = state.run_timer.lock().unwrap();
    if run_timer::is_never_started(&timer) {
        return;
    }
    *timer = run_timer::RunTimerState::default();
    if let Some(path) = run_timer::path(app) {
        run_timer::clear(&path);
    }
    drop(timer);
    let _ = app.emit("run-timer", run_timer::RunTimerState::default());
}

/// Spawns a background tailer at `path` and stores its handle in
/// `AppState.tailer`, replacing (but not stopping) whatever was there —
/// callers that are replacing a live tailer must `take()` and `.stop()`
/// the old handle themselves first. Shared by startup and `apply_settings`.
///
/// `journal_to`, when `Some`, is the session-journal file every
/// *significant* incoming line is appended to (see `journal::is_significant`)
/// so route/session progress can be restored on the next launch. `None`
/// disables journaling (used by the `POE_COPILOT_LOG` dev/demo override,
/// whose fixture lines must not leak into the real session journal).
///
/// Returns `Err` if the poller could not be created (e.g. the file doesn't
/// exist) instead of logging and swallowing the failure itself, so
/// `apply_settings` can propagate it as a failed Save. Startup call sites
/// can't propagate (there's no request to fail), so they log the error
/// themselves and continue without a tailer.
fn spawn_tail(
    app: &tauri::AppHandle,
    path: String,
    start_at_end: bool,
    journal_to: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let poller = input_log::FilePoller::new(path.clone().into(), start_at_end)
        .map_err(|e| format!("cannot open log at {path}: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = input_log::spawn_tailer(poller, std::time::Duration::from_millis(250), tx);

    // Consumer-thread trace (referenced from apply_settings's reorder
    // comment): this thread holds no state across iterations — each pass
    // through the loop re-fetches `state.pipeline` from `app_handle`
    // rather than capturing a specific `Pipeline`, and it terminates on
    // its own once the channel closes (`for line in rx` ends when every
    // `Sender` — here, the one owned by the tailer's producer thread — has
    // been dropped, which `TailerHandle::stop()` guarantees by joining
    // that producer thread before returning). So it never leaks/spins
    // forever, and once `stop()` returns, no *new* lines can arrive on
    // this channel — only whatever was already buffered in-flight before
    // the stop flag took effect. Those last few buffered lines (if any)
    // are still delivered and applied to whatever `state.pipeline`
    // currently holds, which can race a concurrent pipeline swap in
    // `apply_settings`. That's a narrow, accepted residual: reordering
    // stop-before-swap (see apply_settings) closes the large window
    // (the old tailer running and emitting for the pipeline's whole
    // rebuild) down to at most one already-in-flight poll batch, rather
    // than eliminating the race outright, which would need the consumer
    // thread's own join handle threaded back to apply_settings.
    let app_handle = app.clone();
    std::thread::spawn(move || {
        for line in rx {
            // Journal BEFORE feeding the pipeline so a crash mid-update
            // can't lose a line the pipeline already applied. An append
            // failure is logged but never blocks live processing — the
            // overlay keeps working for this session even if persistence
            // is broken (e.g. a read-only config dir).
            if let Some(journal_path) = &journal_to
                && journal::is_significant(&line)
                && let Err(e) = journal::append_line(journal_path, &line)
            {
                diagnostics::diag(&format!("journal: {e}"));
            }
            // Auto-start the run timer on the first zone entry of a run.
            // Costs one extra parse_line per tailed line (the pipeline
            // parses again below); gating on the parse means non-entry
            // lines never touch the run-timer lock.
            if run_timer::is_area_entered(&line) {
                auto_start_run_timer(&app_handle);
            }
            let state: State<AppState> = app_handle.state();
            let model = state.pipeline.lock().unwrap().on_line(&line);
            if let Some(model) = model {
                let _ = app_handle.emit("overlay-model", &model);
            }
        }
    });

    let state: State<AppState> = app.state();
    *state.tailer.lock().unwrap() = Some(handle);
    Ok(())
}

/// Reveals the diagnostics log for the user (tray "Open logs" item). Opens
/// the log file itself when it exists, or falls back to its containing
/// directory when it doesn't (e.g. a fresh install that hasn't logged
/// anything user-relevant yet) — either way the user ends up looking at
/// the right folder in their OS file manager / default text handler.
fn open_logs_impl(app: &tauri::AppHandle) {
    let Some(path) = diagnostics::log_path(app) else {
        diagnostics::diag("open-logs: could not resolve diagnostics log path");
        return;
    };
    let target = if path.exists() {
        path.clone()
    } else {
        match path.parent() {
            Some(dir) => dir.to_path_buf(),
            None => path.clone(),
        }
    };
    if let Err(e) = app
        .opener()
        .open_path(target.to_string_lossy().into_owned(), None::<&str>)
    {
        diagnostics::diag(&format!(
            "open-logs: failed to open {}: {e}",
            target.display()
        ));
    }
}

fn main() {
    diagnostics::install_panic_hook();
    tauri::Builder::default()
        // Position-only: persisting SIZE would let a relaunch pick up the
        // zoomed height from a previous session instead of always starting
        // at the compact bar height.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .build(),
        )
        // Shortcuts are no longer registered at builder time: they come
        // from config (user-rebindable), so `.setup()` registers them via
        // `hotkeys::register_all` and `apply_settings` re-registers live
        // on rebind. Each registration carries its own per-shortcut
        // handler (`on_shortcut`), so no builder-level handler either.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_model,
            set_setup_mode,
            toggle_zoom,
            toggle_compact,
            toggle_hide,
            get_config,
            pick_log_file,
            import_pob,
            apply_settings,
            open_settings,
            set_overlay_opacity,
            set_overlay_height,
            get_run_timer
        ])
        .setup(|app| {
            // Diagnostics init MUST run first, before anything else in
            // `.setup()` that might log: everything below this line goes
            // through `diagnostics::diag`, and until the log path is
            // resolved here, `diag`/`write` calls are stderr-only (see
            // `diagnostics::write`'s `OnceLock` guard) — which is exactly
            // the gap this ordering closes.
            diagnostics::init(app.handle());
            diagnostics::diag(&format!("app starting v{}", env!("CARGO_PKG_VERSION")));

            // Packaged-build data root detection: MUST run before anything
            // below that can trigger a `content::*_dir()` lookup (the
            // config-load-triggered `parse_pob_code` call a few lines down
            // loads vendored gem data; `Pipeline::new` further down loads
            // the full vendored game data and layouts). `data_root::
            // set_data_root` is a set-once `OnceLock` — once any lookup
            // has happened, later calls can't relocate data mid-run — so
            // this has to be the first thing `.setup()` does.
            //
            // A bundled install ships `vendor/` and `content/layouts/` as
            // Tauri resources (see `tauri.conf.json`'s `bundle.resources`),
            // copied next to the resolved resource dir. Detecting
            // `resource_dir/vendor/exile-leveling` is how we tell a
            // packaged run (resources present) apart from `tauri dev`
            // (resource dir has no `vendor/`) — the latter falls through
            // to `content::data_root`'s repo-root default untouched.
            if let Ok(resource_dir) = app.path().resource_dir()
                && resource_dir.join("vendor").join("exile-leveling").is_dir()
                && let Err(rejected) = content::data_root::set_data_root(resource_dir.clone())
            {
                diagnostics::diag(&format!("data root already set; ignoring {rejected:?}"));
            }

            // Config load and initial Pipeline construction happen here
            // (rather than before the Builder chain) because building the
            // real pipeline needs the resolved variant/build from the
            // config file, which in turn needs an AppHandle to locate the
            // app config dir.
            let cfg = config::load(app.handle());
            let variant = map_variant(&cfg.variant).unwrap_or_else(|e| {
                diagnostics::diag(&format!("config: {e}; falling back to league-start"));
                Variant::LeagueStart
            });
            let build = cfg
                .pob_code
                .as_deref()
                .and_then(|code| match parse_pob_code(code) {
                    Ok(plan) => Some(plan),
                    Err(e) => {
                        diagnostics::diag(&format!("config: failed to parse saved PoB code: {e}"));
                        None
                    }
                });
            let mut pipeline = Pipeline::new(variant, build).expect("content data must load");

            // Restore the previous session's route/session progress from
            // the on-disk journal BEFORE the state is managed (and thus
            // before the frontend's first `get_model` can observe it), so
            // a relaunch resumes at the last-known act/zone/route step
            // instead of dropping back to the "waiting for Client.txt"
            // state. A missing/corrupt/oversized journal yields no lines
            // and the pipeline simply starts fresh.
            if let Some(journal_path) = journal::journal_path(app.handle()) {
                let lines = journal::load_lines(&journal_path, journal::MAX_JOURNAL_BYTES);
                if !lines.is_empty() {
                    let fed = journal::replay_into(&mut pipeline, &lines);
                    diagnostics::diag(&format!(
                        "journal: restored session progress from {fed} journaled lines"
                    ));
                }
                // Compact an over-target journal down to the newest suffix
                // of what was just replayed, so the append-only file can't
                // ratchet toward (and eventually past) the read cap across
                // long-lived configs. Runs before the tailer spawns, so no
                // concurrent appends can race the rewrite.
                match journal::compact(&journal_path, &lines, journal::COMPACT_TARGET_BYTES) {
                    Ok(true) => diagnostics::diag("journal: compacted oversized session journal"),
                    Ok(false) => {}
                    Err(e) => diagnostics::diag(&format!("journal: {e}")),
                }
            }

            app.manage(AppState {
                pipeline: Mutex::new(pipeline),
                setup_mode: Mutex::new(false),
                zoom: Mutex::new(false),
                compact: Mutex::new(false),
                hidden: Mutex::new(false),
                setup_item: Mutex::new(None),
                zoom_item: Mutex::new(None),
                compact_item: Mutex::new(None),
                hide_item: Mutex::new(None),
                tailer: Mutex::new(None),
                run_timer: Mutex::new(
                    run_timer::path(app.handle())
                        .map(|p| run_timer::load(&p))
                        .unwrap_or_default(),
                ),
            });

            // Global hotkeys from config. A failure here (combo grabbed by
            // another app, or an invalid combo hand-edited into config.json)
            // must not abort startup — log it and fall back to the default
            // bindings so the overlay stays controllable.
            if let Err(e) = hotkeys::register_all(app.handle(), &cfg.hotkeys, dispatch_hotkey) {
                diagnostics::diag(&format!("hotkeys: {e}"));
                let defaults = hotkeys::HotkeyConfig::default();
                if cfg.hotkeys != defaults {
                    diagnostics::diag("hotkeys: falling back to default bindings");
                    if let Err(e2) = hotkeys::register_all(app.handle(), &defaults, dispatch_hotkey)
                    {
                        diagnostics::diag(&format!("hotkeys: default bindings also failed: {e2}"));
                    }
                }
            }

            // Tray menu
            let setup_item =
                CheckMenuItem::with_id(app, "setup", "Setup Mode", true, false, None::<&str>)?;
            let zoom_item = CheckMenuItem::with_id(app, "zoom", "Zoom", true, false, None::<&str>)?;
            let compact_item =
                CheckMenuItem::with_id(app, "compact", "Compact mode", true, false, None::<&str>)?;
            let hide_item =
                CheckMenuItem::with_id(app, "hide", "Hide overlay", true, false, None::<&str>)?;
            let reset_timer_item =
                MenuItem::with_id(app, "reset-timer", "Reset run timer", true, None::<&str>)?;
            let open_logs_item =
                MenuItem::with_id(app, "open-logs", "Open logs", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &setup_item,
                    &zoom_item,
                    &compact_item,
                    &hide_item,
                    &reset_timer_item,
                    &open_logs_item,
                    &settings_item,
                    &quit_item,
                ],
            )?;

            let state: State<AppState> = app.state();
            *state.setup_item.lock().unwrap() = Some(setup_item.clone());
            *state.zoom_item.lock().unwrap() = Some(zoom_item.clone());
            *state.compact_item.lock().unwrap() = Some(compact_item.clone());
            *state.hide_item.lock().unwrap() = Some(hide_item.clone());
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
                    "compact" => {
                        toggle_compact_impl(app);
                    }
                    "hide" => {
                        toggle_hide_impl(app);
                    }
                    "reset-timer" => {
                        reset_run_timer_impl(app);
                    }
                    "open-logs" => {
                        open_logs_impl(app);
                    }
                    "settings" => {
                        open_settings_window(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Click-through by default (play mode).
            if let Some(win) = app.get_webview_window("main")
                && let Err(e) = win.set_ignore_cursor_events(true)
            {
                diagnostics::diag(&format!("setup: failed to set ignore-cursor-events: {e}"));
            }

            // Tailer startup: POE_COPILOT_LOG (dev/demo override) beats the
            // configured client_log_path, which beats no tailer at all
            // (waiting state, until Settings configures a path).
            if let Ok(log_path) = std::env::var("POE_COPILOT_LOG") {
                // Real Client.txt files are huge, append-only histories
                // going back to whenever the character was created, so
                // the tailer defaults to starting at end-of-file (skip
                // the backlog, only react to new lines). The fake-play
                // demo harness instead writes a small fixture log from
                // scratch and needs the tailer to read it from byte 0,
                // so POE_COPILOT_LOG_REPLAY=1 flips that default for
                // local development/demo runs only.
                let start_at_end = std::env::var("POE_COPILOT_LOG_REPLAY").as_deref() != Ok("1");
                // No journaling for the dev/demo override: fixture lines
                // must not contaminate the real persisted session journal.
                if let Err(e) = spawn_tail(app.handle(), log_path, start_at_end, None) {
                    diagnostics::diag(&format!("tailer: {e}"));
                }
            } else if let Some(log_path) = cfg.client_log_path.clone() {
                let journal_to = journal::journal_path(app.handle());
                if let Err(e) = spawn_tail(app.handle(), log_path, true, journal_to) {
                    diagnostics::diag(&format!("tailer: {e}"));
                }
            } else {
                diagnostics::diag("no client log configured — overlay will wait for Settings");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_height_in_range_accepts_bounds_and_interior() {
        assert!(overlay_height_in_range(MIN_OVERLAY_HEIGHT));
        assert!(overlay_height_in_range(MAX_OVERLAY_HEIGHT));
        assert!(overlay_height_in_range(150.0));
    }

    #[test]
    fn overlay_height_in_range_rejects_out_of_range_and_non_finite() {
        assert!(!overlay_height_in_range(MIN_OVERLAY_HEIGHT - 0.1));
        assert!(!overlay_height_in_range(MAX_OVERLAY_HEIGHT + 0.1));
        assert!(!overlay_height_in_range(f64::NAN));
        assert!(!overlay_height_in_range(f64::INFINITY));
        assert!(!overlay_height_in_range(-1.0));
    }

    #[test]
    fn is_regular_file_true_for_a_plain_file() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-is-regular-file-test-{}.txt",
            std::process::id()
        ));
        std::fs::write(&path, "Client.txt content").unwrap();
        assert!(is_regular_file(path.to_str().unwrap()));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn is_regular_file_false_for_a_directory() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-is-regular-file-test-dir-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        assert!(!is_regular_file(path.to_str().unwrap()));
        std::fs::remove_dir(&path).unwrap();
    }

    #[test]
    fn is_regular_file_false_for_a_missing_path() {
        assert!(!is_regular_file("/definitely/does/not/exist/Client.txt"));
    }
}
