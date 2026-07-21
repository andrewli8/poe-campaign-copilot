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
    if let Some(win) = app.get_webview_window("main")
        && let Ok(size) = win.outer_size()
    {
        let height = if new_zoom { 420 } else { 150 };
        let _ = win.set_size(tauri::PhysicalSize::new(size.width, height));
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
        .invoke_handler(tauri::generate_handler![
            get_model,
            set_setup_mode,
            toggle_zoom
        ])
        .setup(|app| {
            // Tray menu
            let setup_item =
                CheckMenuItem::with_id(app, "setup", "Setup Mode", true, false, None::<&str>)?;
            let zoom_item = CheckMenuItem::with_id(app, "zoom", "Zoom", true, false, None::<&str>)?;
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
                    let _tailer =
                        input_log::spawn_tailer(poller, std::time::Duration::from_millis(250), tx);
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
