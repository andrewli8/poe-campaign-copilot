//! User-configurable global hotkeys. The canonical combo format is the
//! lowercase "mod+mod+key" grammar `tauri-plugin-global-shortcut` parses
//! natively ("alt+shift+s"); the settings UI (src/hotkeys.ts) produces
//! exactly this form, and `validate` re-checks it server-side on Save.
//!
//! Kept as its own module (rather than folded into config.rs/main.rs) so
//! parsing/validation stay pure and unit-testable without an AppHandle,
//! and to keep the surface area other agents touch in those files small.

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub fn default_zoom_hotkey() -> String {
    "alt+shift+z".to_string()
}
pub fn default_compact_hotkey() -> String {
    "alt+shift+c".to_string()
}
pub fn default_hide_hotkey() -> String {
    "alt+shift+h".to_string()
}
pub fn default_setup_hotkey() -> String {
    "alt+shift+s".to_string()
}
pub fn default_settings_hotkey() -> String {
    "alt+shift+o".to_string()
}
pub fn default_timer_hotkey() -> String {
    "alt+shift+t".to_string()
}

/// Per-field serde defaults so an old (or partially hand-edited)
/// config.json that omits any of these keys still loads with the
/// original bindings intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_zoom_hotkey")]
    pub zoom: String,
    #[serde(default = "default_compact_hotkey")]
    pub compact: String,
    #[serde(default = "default_hide_hotkey")]
    pub hide: String,
    #[serde(default = "default_setup_hotkey")]
    pub setup: String,
    #[serde(default = "default_settings_hotkey")]
    pub settings: String,
    #[serde(default = "default_timer_hotkey")]
    pub timer: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            zoom: default_zoom_hotkey(),
            compact: default_compact_hotkey(),
            hide: default_hide_hotkey(),
            setup: default_setup_hotkey(),
            settings: default_settings_hotkey(),
            timer: default_timer_hotkey(),
        }
    }
}

/// The actions a global hotkey can trigger. Dispatch itself lives in
/// main.rs (`dispatch_hotkey`), which owns the toggle implementations —
/// this module only knows the action names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Zoom,
    Compact,
    Hide,
    Setup,
    Settings,
    Timer,
}

impl HotkeyAction {
    fn label(self) -> &'static str {
        match self {
            HotkeyAction::Zoom => "toggle zoom",
            HotkeyAction::Compact => "toggle compact mode",
            HotkeyAction::Hide => "hide/show overlay",
            HotkeyAction::Setup => "toggle setup mode",
            HotkeyAction::Settings => "open settings",
            HotkeyAction::Timer => "start/stop run timer",
        }
    }
}

/// Every (action, combo) pair in a fixed order.
pub fn bindings(cfg: &HotkeyConfig) -> [(HotkeyAction, &str); 6] {
    [
        (HotkeyAction::Zoom, cfg.zoom.as_str()),
        (HotkeyAction::Compact, cfg.compact.as_str()),
        (HotkeyAction::Hide, cfg.hide.as_str()),
        (HotkeyAction::Setup, cfg.setup.as_str()),
        (HotkeyAction::Settings, cfg.settings.as_str()),
        (HotkeyAction::Timer, cfg.timer.as_str()),
    ]
}

/// Parses a combo string via the plugin's own parser, so "valid" here is
/// exactly "registrable there". The error names the offending combo for
/// direct display in the settings UI.
pub fn parse_shortcut(combo: &str) -> Result<Shortcut, String> {
    combo
        .parse::<Shortcut>()
        .map_err(|e| format!("invalid hotkey {combo:?}: {e}"))
}

/// Validates a whole config: every combo must parse, and no two actions
/// may resolve to the same physical chord (compared on the PARSED
/// shortcut, so "shift+alt+H" vs "alt+shift+h" is still a conflict).
pub fn validate(cfg: &HotkeyConfig) -> Result<(), String> {
    let mut seen: Vec<(Shortcut, HotkeyAction, &str)> = Vec::new();
    for (action, combo) in bindings(cfg) {
        let shortcut = parse_shortcut(combo)?;
        if let Some((_, other_action, _)) = seen.iter().find(|(s, _, _)| *s == shortcut) {
            return Err(format!(
                "hotkey conflict: {combo:?} is bound to both \"{}\" and \"{}\"",
                other_action.label(),
                action.label()
            ));
        }
        seen.push((shortcut, action, combo));
    }
    Ok(())
}

/// Function-pointer dispatch keeps this module free of main.rs's toggle
/// implementations (and trivially `Send + Sync` for the handler closure).
pub type Dispatch = fn(&tauri::AppHandle, HotkeyAction);

/// Registers every configured hotkey, validating the whole set first.
/// All-or-nothing: if any single registration fails (e.g. the combo is
/// already taken by the OS or another app), everything registered so far
/// in this call is unregistered again and the error is returned, so the
/// caller can revert to a previous binding set cleanly.
pub fn register_all(
    app: &tauri::AppHandle,
    cfg: &HotkeyConfig,
    dispatch: Dispatch,
) -> Result<(), String> {
    validate(cfg)?;
    let mut registered: Vec<Shortcut> = Vec::new();
    for (action, combo) in bindings(cfg) {
        let shortcut = parse_shortcut(combo)?;
        let result = app
            .global_shortcut()
            .on_shortcut(shortcut, move |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    dispatch(app, action);
                }
            });
        if let Err(e) = result {
            for s in registered {
                if let Err(e2) = app.global_shortcut().unregister(s) {
                    eprintln!("hotkeys: rollback unregister failed: {e2}");
                }
            }
            return Err(format!(
                "could not register hotkey {combo:?} for \"{}\" (it may be in use by another application): {e}",
                action.label()
            ));
        }
        registered.push(shortcut);
    }
    Ok(())
}

/// Unregisters every hotkey in `cfg`. Best-effort: an unparseable or
/// never-registered combo is skipped/logged, never fatal — this runs on
/// the teardown side of a rebind, where the new bindings matter more.
pub fn unregister_all(app: &tauri::AppHandle, cfg: &HotkeyConfig) {
    for (_, combo) in bindings(cfg) {
        match parse_shortcut(combo) {
            Ok(shortcut) => {
                if let Err(e) = app.global_shortcut().unregister(shortcut) {
                    eprintln!("hotkeys: failed to unregister {combo:?}: {e}");
                }
            }
            Err(e) => eprintln!("hotkeys: skipping unregister of {combo:?}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_frontend_defaults() {
        let cfg = HotkeyConfig::default();
        assert_eq!(cfg.zoom, "alt+shift+z");
        assert_eq!(cfg.compact, "alt+shift+c");
        assert_eq!(cfg.hide, "alt+shift+h");
        assert_eq!(cfg.setup, "alt+shift+s");
        assert_eq!(cfg.settings, "alt+shift+o");
        assert_eq!(cfg.timer, "alt+shift+t");
    }

    #[test]
    fn defaults_are_valid_and_conflict_free() {
        assert!(validate(&HotkeyConfig::default()).is_ok());
    }

    #[test]
    fn parse_accepts_the_canonical_forms_the_ui_produces() {
        for combo in [
            "alt+shift+z",
            "ctrl+shift+1",
            "ctrl+f12",
            "alt+space",
            "ctrl+alt+shift+p",
            "alt+up",
        ] {
            assert!(parse_shortcut(combo).is_ok(), "should parse: {combo}");
        }
    }

    #[test]
    fn parse_rejects_garbage_with_a_clear_message() {
        let err = parse_shortcut("not a combo").unwrap_err();
        assert!(
            err.contains("not a combo"),
            "message names the combo: {err}"
        );
        assert!(parse_shortcut("").is_err());
        assert!(parse_shortcut("alt+shift+bogus").is_err());
    }

    #[test]
    fn validate_rejects_two_actions_sharing_a_combo() {
        let cfg = HotkeyConfig {
            setup: "alt+shift+h".into(),
            ..HotkeyConfig::default()
        };
        let err = validate(&cfg).unwrap_err();
        assert!(
            err.contains("alt+shift+h"),
            "conflict message names the combo: {err}"
        );
    }

    #[test]
    fn validate_detects_conflicts_across_formatting_differences() {
        // Same physical chord, different spelling/order — still a conflict.
        let cfg = HotkeyConfig {
            settings: "shift+alt+H".into(),
            ..HotkeyConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn validate_rejects_an_unparseable_combo() {
        let cfg = HotkeyConfig {
            hide: "definitely not a hotkey".into(),
            ..HotkeyConfig::default()
        };
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn missing_hotkey_fields_deserialize_to_defaults() {
        // Old configs (or partially hand-edited ones) omit fields; serde
        // per-field defaults must fill them in.
        let cfg: HotkeyConfig = serde_json::from_str(r#"{"settings":"ctrl+shift+o"}"#).unwrap();
        assert_eq!(cfg.settings, "ctrl+shift+o");
        assert_eq!(cfg.zoom, "alt+shift+z");
        assert_eq!(cfg.setup, "alt+shift+s");
    }

    #[test]
    fn hotkey_config_round_trips_through_json() {
        let cfg = HotkeyConfig {
            zoom: "ctrl+shift+z".into(),
            ..HotkeyConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: HotkeyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn bindings_covers_every_action_once() {
        let cfg = HotkeyConfig::default();
        let combos: Vec<&str> = bindings(&cfg).iter().map(|(_, c)| *c).collect();
        assert_eq!(
            combos,
            vec![
                "alt+shift+z",
                "alt+shift+c",
                "alt+shift+h",
                "alt+shift+s",
                "alt+shift+o",
                "alt+shift+t"
            ]
        );
    }

    #[test]
    fn missing_timer_field_deserializes_to_default() {
        // Configs written before the run-timer feature omit the key.
        let cfg: HotkeyConfig = serde_json::from_str(r#"{"settings":"ctrl+shift+o"}"#).unwrap();
        assert_eq!(cfg.timer, "alt+shift+t");
    }
}
