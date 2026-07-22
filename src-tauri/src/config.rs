//! Persisted app configuration: the Client.txt path, the route variant, and
//! an optional Path of Building share code. Loaded once at startup and
//! rewritten whenever the settings UI calls `apply_settings`.

use serde::{Deserialize, Serialize};

pub fn default_variant() -> String {
    "league-start".to_string()
}

const KNOWN_VARIANTS: [&str; 2] = ["league-start", "standard"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub client_log_path: Option<String>,
    #[serde(default = "default_variant")]
    pub variant: String,
    pub pob_code: Option<String>,
}

/// Pure parse: any JSON that doesn't deserialize into `AppConfig` (missing
/// file content, corrupt/garbage text, wrong shape) degrades to
/// `AppConfig::default()` rather than erroring. A JSON object that's simply
/// missing the `variant` key still parses fine and picks up
/// `default_variant()` via serde's per-field default.
///
/// `load()` doesn't call this directly — it needs the `Result` (not just
/// the collapsed `AppConfig`) so it can log a corrupt-file warning without
/// parsing the JSON twice, so it shares `try_parse` with this function
/// instead. Kept as its own pure, `Result`-free function because that's
/// the shape unit tests (and any future non-I/O consumer, e.g. a settings
/// preview) want; not currently called from any non-test code path, hence
/// the explicit `allow` rather than a misleading production call site.
#[allow(dead_code)]
pub fn parse_config(json: &str) -> AppConfig {
    try_parse(json).unwrap_or_default()
}

fn try_parse(json: &str) -> Result<AppConfig, serde_json::Error> {
    serde_json::from_str(json)
}

/// Pure serialize: pretty-printed so a hand-edited config.json stays
/// readable.
pub fn config_json(cfg: &AppConfig) -> String {
    serde_json::to_string_pretty(cfg).unwrap_or_default()
}

/// Returns a copy of `cfg` with an unrecognized `variant` value replaced by
/// `default_variant()` — used by `load()` so a corrupt or hand-edited
/// config.json with a bogus/stale variant (e.g. from a removed route
/// variant) degrades gracefully to the default rather than getting
/// rejected wholesale by `map_variant` at pipeline-build time. Returns
/// `cfg` unchanged (no clone-and-replace) when the variant is already
/// known.
fn normalize_variant(cfg: AppConfig) -> AppConfig {
    if KNOWN_VARIANTS.contains(&cfg.variant.as_str()) {
        cfg
    } else {
        AppConfig {
            variant: default_variant(),
            ..cfg
        }
    }
}

/// True when every persisted field of `a` and `b` matches. Used by
/// `apply_settings` to detect a no-op Save (e.g. re-opening Settings and
/// clicking Save without changing anything) so it can skip the
/// pipeline/tailer rebuild entirely — rebuilding on a no-op Save would
/// otherwise reset in-progress route/task state and the player's pinned
/// level for no reason.
pub fn configs_equal(a: &AppConfig, b: &AppConfig) -> bool {
    a.client_log_path == b.client_log_path && a.variant == b.variant && a.pob_code == b.pob_code
}

fn config_path(app: &tauri::AppHandle) -> tauri::Result<std::path::PathBuf> {
    use tauri::Manager;
    Ok(app.path().app_config_dir()?.join("config.json"))
}

/// Loads the config file, degrading to `AppConfig::default()` (and logging
/// to stderr) on anything but a plain "file does not exist yet" — which is
/// the expected, silent state on first run.
pub fn load(app: &tauri::AppHandle) -> AppConfig {
    let path = match config_path(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("config: could not resolve app config dir: {e}");
            return AppConfig::default();
        }
    };
    let json = match std::fs::read_to_string(&path) {
        Ok(j) => j,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AppConfig::default(),
        Err(e) => {
            eprintln!("config: failed to read {}: {e}", path.display());
            return AppConfig::default();
        }
    };
    // Single parse: log from this same Result rather than parsing twice
    // (once to check for an error, again via parse_config to get the
    // value).
    let parsed = try_parse(&json);
    if let Err(e) = &parsed {
        eprintln!("config: corrupt config at {}: {e}", path.display());
    }
    let cfg = parsed.unwrap_or_default();
    if !KNOWN_VARIANTS.contains(&cfg.variant.as_str()) {
        eprintln!(
            "config: unknown route variant {:?} at {}; normalizing to {}",
            cfg.variant,
            path.display(),
            default_variant()
        );
    }
    normalize_variant(cfg)
}

/// Saves the config file, creating the app config directory if needed.
pub fn save(app: &tauri::AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let path = config_path(app).map_err(|e| format!("could not resolve app config dir: {e}"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create config dir: {e}"))?;
    }
    std::fs::write(&path, config_json(cfg)).map_err(|e| format!("could not write config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_json_degrades_to_default() {
        let cfg = parse_config("not json at all");
        assert_eq!(cfg.client_log_path, None);
        assert_eq!(cfg.pob_code, None);
        assert_eq!(cfg.variant, String::default());
    }

    #[test]
    fn missing_variant_key_defaults_to_league_start() {
        let cfg = parse_config(r#"{"client_log_path":"/tmp/Client.txt"}"#);
        assert_eq!(cfg.variant, "league-start");
        assert_eq!(cfg.client_log_path.as_deref(), Some("/tmp/Client.txt"));
    }

    #[test]
    fn round_trips_through_json() {
        let cfg = AppConfig {
            client_log_path: Some("/tmp/Client.txt".into()),
            variant: "standard".into(),
            pob_code: Some("code".into()),
        };
        let json = config_json(&cfg);
        let parsed = parse_config(&json);
        assert_eq!(parsed.client_log_path, cfg.client_log_path);
        assert_eq!(parsed.variant, cfg.variant);
        assert_eq!(parsed.pob_code, cfg.pob_code);
    }

    #[test]
    fn empty_object_still_defaults_variant() {
        let cfg = parse_config("{}");
        assert_eq!(cfg.variant, "league-start");
    }

    #[test]
    fn normalize_variant_replaces_unknown_values() {
        let cfg = AppConfig {
            client_log_path: Some("/tmp/Client.txt".into()),
            variant: "hardcore-solo-self-found".into(),
            pob_code: None,
        };
        let normalized = normalize_variant(cfg);
        assert_eq!(normalized.variant, "league-start");
        assert_eq!(
            normalized.client_log_path.as_deref(),
            Some("/tmp/Client.txt"),
            "other fields are preserved"
        );
    }

    #[test]
    fn normalize_variant_leaves_known_values_untouched() {
        let cfg = AppConfig {
            client_log_path: None,
            variant: "standard".into(),
            pob_code: None,
        };
        assert_eq!(normalize_variant(cfg).variant, "standard");
    }

    #[test]
    fn configs_equal_compares_all_three_fields() {
        let a = AppConfig {
            client_log_path: Some("/tmp/Client.txt".into()),
            variant: "standard".into(),
            pob_code: Some("code".into()),
        };
        let same = a.clone();
        assert!(configs_equal(&a, &same));

        let different_path = AppConfig {
            client_log_path: Some("/tmp/Other.txt".into()),
            ..a.clone()
        };
        assert!(!configs_equal(&a, &different_path));

        let different_variant = AppConfig {
            variant: "league-start".into(),
            ..a.clone()
        };
        assert!(!configs_equal(&a, &different_variant));

        let different_code = AppConfig {
            pob_code: None,
            ..a.clone()
        };
        assert!(!configs_equal(&a, &different_code));
    }
}
