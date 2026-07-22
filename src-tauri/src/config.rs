//! Persisted app configuration: the Client.txt path, the route variant, and
//! an optional Path of Building share code. Loaded once at startup and
//! rewritten whenever the settings UI calls `apply_settings`.

use serde::{Deserialize, Serialize};

pub fn default_variant() -> String {
    "league-start".to_string()
}

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
    parsed.unwrap_or_default()
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
}
