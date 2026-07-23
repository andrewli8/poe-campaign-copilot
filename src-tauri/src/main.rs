#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod pipeline;

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
use tauri_plugin_window_state::StateFlags;

struct AppState {
    pipeline: Mutex<Pipeline>,
    setup_mode: Mutex<bool>,
    zoom: Mutex<bool>,
    hidden: Mutex<bool>,
    setup_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    zoom_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    hide_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    /// The currently running Client.txt tailer, if any (`None` while
    /// waiting for a log path to be configured). Swapped out by
    /// `apply_settings` whenever the configured path changes.
    tailer: Mutex<Option<TailerHandle>>,
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
fn toggle_hide(app: tauri::AppHandle) -> bool {
    toggle_hide_impl(&app)
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> AppConfig {
    config::load(&app)
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
    // (0) No-op guard: if the incoming config is byte-for-byte identical to
    // what's already persisted, skip the rebuild entirely and report
    // success. Settings can be reopened and Saved without changing
    // anything (or Save can be double-clicked), and rebuilding the
    // pipeline/tailer on every Save — even a no-op one — would tear down
    // and recreate route-engine/task-engine state mid-run, resetting route
    // progress and un-pinning the player's level for no reason.
    if config::configs_equal(&config::load(&app), &cfg) {
        return Ok(());
    }

    // (a) Validate first: nothing below should mutate shared state or the
    // config file on disk if the submitted settings don't even parse.
    let variant = map_variant(&cfg.variant)?;
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

    // (e) Spawn the new tailer at the configured path. A failure here
    // (e.g. the configured path was removed between validation above and
    // now) propagates as an error rather than being swallowed: the caller
    // ordering below means config::save (f) never runs in that case, so a
    // Save that can't actually start tailing doesn't get persisted as if
    // it had succeeded.
    if let Some(path) = cfg.client_log_path.clone() {
        spawn_tail(&app, path, true)?;
    }

    // (f) Persist only after the rebuild above fully succeeded.
    config::save(&app, &cfg)?;

    // (g) Emit a fresh model with no lock held.
    let model = {
        let state: State<AppState> = app.state();
        state.pipeline.lock().unwrap().current_model()
    };
    let _ = app.emit("overlay-model", &model);
    Ok(())
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    open_settings_window(&app);
}

fn open_settings_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        if let Err(e) = win.set_focus() {
            eprintln!("open_settings: failed to focus existing window: {e}");
        }
        return;
    }
    let result = tauri::WebviewWindowBuilder::new(
        app,
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
        eprintln!("open_settings: failed to create settings window: {e}");
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
            eprintln!("apply_setup_mode: failed to set ignore-cursor-events: {e}");
        }
        let _ = win.set_resizable(enabled);
    }
    if let Some(item) = state.setup_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(enabled);
    }
    let _ = app.emit("setup-mode", enabled);
}

fn toggle_zoom_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let mut zoom = state.zoom.lock().unwrap();
    *zoom = !*zoom;
    let new_zoom = *zoom;
    // Hold the zoom guard across the resize so a second toggle_zoom call
    // (e.g. rapid tray clicks or the global shortcut firing twice) can't
    // interleave with set_size and leave the window size out of sync with
    // the `zoom` flag it's supposed to reflect.
    if let Some(win) = app.get_webview_window("main")
        && let Ok(scale) = win.scale_factor()
        && let Ok(size) = win.outer_size()
    {
        let logical = size.to_logical::<f64>(scale);
        let height = if new_zoom { 420.0 } else { 150.0 };
        if let Err(e) = win.set_size(tauri::LogicalSize::new(logical.width, height)) {
            eprintln!("toggle_zoom: failed to resize window: {e}");
        }
    }
    drop(zoom);
    if let Some(item) = state.zoom_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_zoom);
    }
    let _ = app.emit("zoom", new_zoom);
    new_zoom
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
                eprintln!("toggle_hide: failed to hide window: {e}");
            }
        } else {
            if let Err(e) = win.show() {
                eprintln!("toggle_hide: failed to show window: {e}");
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
                eprintln!("toggle_hide: failed to restore ignore-cursor-events: {e}");
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

/// Spawns a background tailer at `path` and stores its handle in
/// `AppState.tailer`, replacing (but not stopping) whatever was there —
/// callers that are replacing a live tailer must `take()` and `.stop()`
/// the old handle themselves first. Shared by startup and `apply_settings`.
///
/// Returns `Err` if the poller could not be created (e.g. the file doesn't
/// exist) instead of logging and swallowing the failure itself, so
/// `apply_settings` can propagate it as a failed Save. Startup call sites
/// can't propagate (there's no request to fail), so they log the error
/// themselves and continue without a tailer.
fn spawn_tail(app: &tauri::AppHandle, path: String, start_at_end: bool) -> Result<(), String> {
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

fn main() {
    tauri::Builder::default()
        // Position-only: persisting SIZE would let a relaunch pick up the
        // zoomed height from a previous session instead of always starting
        // at the compact bar height.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+shift+z", "alt+shift+h"])
                .expect("valid shortcut")
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::{Code, Modifiers};
                    if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
                    if shortcut.matches(alt_shift, Code::KeyZ) {
                        toggle_zoom_impl(app);
                    } else if shortcut.matches(alt_shift, Code::KeyH) {
                        toggle_hide_impl(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_model,
            set_setup_mode,
            toggle_zoom,
            toggle_hide,
            get_config,
            pick_log_file,
            import_pob,
            apply_settings,
            open_settings
        ])
        .setup(|app| {
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
                eprintln!("data root already set; ignoring {rejected:?}");
            }

            // Config load and initial Pipeline construction happen here
            // (rather than before the Builder chain) because building the
            // real pipeline needs the resolved variant/build from the
            // config file, which in turn needs an AppHandle to locate the
            // app config dir.
            let cfg = config::load(app.handle());
            let variant = map_variant(&cfg.variant).unwrap_or_else(|e| {
                eprintln!("config: {e}; falling back to league-start");
                Variant::LeagueStart
            });
            let build = cfg
                .pob_code
                .as_deref()
                .and_then(|code| match parse_pob_code(code) {
                    Ok(plan) => Some(plan),
                    Err(e) => {
                        eprintln!("config: failed to parse saved PoB code: {e}");
                        None
                    }
                });
            let pipeline = Pipeline::new(variant, build).expect("content data must load");

            app.manage(AppState {
                pipeline: Mutex::new(pipeline),
                setup_mode: Mutex::new(false),
                zoom: Mutex::new(false),
                hidden: Mutex::new(false),
                setup_item: Mutex::new(None),
                zoom_item: Mutex::new(None),
                hide_item: Mutex::new(None),
                tailer: Mutex::new(None),
            });

            // Tray menu
            let setup_item =
                CheckMenuItem::with_id(app, "setup", "Setup Mode", true, false, None::<&str>)?;
            let zoom_item = CheckMenuItem::with_id(app, "zoom", "Zoom", true, false, None::<&str>)?;
            let hide_item =
                CheckMenuItem::with_id(app, "hide", "Hide overlay", true, false, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &setup_item,
                    &zoom_item,
                    &hide_item,
                    &settings_item,
                    &quit_item,
                ],
            )?;

            let state: State<AppState> = app.state();
            *state.setup_item.lock().unwrap() = Some(setup_item.clone());
            *state.zoom_item.lock().unwrap() = Some(zoom_item.clone());
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
                    "hide" => {
                        toggle_hide_impl(app);
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
                eprintln!("setup: failed to set ignore-cursor-events: {e}");
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
                if let Err(e) = spawn_tail(app.handle(), log_path, start_at_end) {
                    eprintln!("tailer: {e}");
                }
            } else if let Some(log_path) = cfg.client_log_path.clone() {
                if let Err(e) = spawn_tail(app.handle(), log_path, true) {
                    eprintln!("tailer: {e}");
                }
            } else {
                eprintln!("no client log configured — overlay will wait for Settings");
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
