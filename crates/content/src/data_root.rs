//! Process-wide data root resolution, allowing packaged builds to relocate
//! vendored content and layouts away from the source repository.

use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Environment variable that overrides the data root at runtime, checked on
/// every call. Intended for diagnostics/tests and packaged-build overrides.
pub const DATA_ROOT_ENV_VAR: &str = "POE_COPILOT_DATA_ROOT";

static DATA_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Sets the process-wide data root exactly once. Returns `Err(root)` with
/// the rejected value if a root has already been set by a previous call.
///
/// The app must call this (if at all) before any code path looks up
/// `data_root()` (or the crate's `*_dir()` helpers) — once a lookup has
/// happened, later calls to `set_data_root` cannot relocate data mid-run.
pub fn set_data_root(root: PathBuf) -> Result<(), PathBuf> {
    DATA_ROOT.set(root)
}

/// Resolves the data root: `POE_COPILOT_DATA_ROOT` env var (read fresh on
/// every call) takes precedence over the `OnceLock` value set via
/// `set_data_root`, which takes precedence over the compile-time default
/// (the repo root, `CARGO_MANIFEST_DIR/../..`).
pub fn data_root() -> PathBuf {
    let env_override = env::var_os(DATA_ROOT_ENV_VAR).map(PathBuf::from);
    resolve(env_override, DATA_ROOT.get())
}

fn default_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Pure precedence resolution, exercised directly by tests to avoid
/// environment-variable races across the threaded test runner.
fn resolve(env_override: Option<PathBuf>, once: Option<&PathBuf>) -> PathBuf {
    env_override
        .or_else(|| once.cloned())
        .unwrap_or_else(default_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_env_over_once() {
        let env_path = PathBuf::from("/tmp/env-root");
        let once_path = PathBuf::from("/tmp/once-root");
        assert_eq!(resolve(Some(env_path.clone()), Some(&once_path)), env_path);
    }

    #[test]
    fn resolve_falls_back_to_once_without_env() {
        let once_path = PathBuf::from("/tmp/once-root");
        assert_eq!(resolve(None, Some(&once_path)), once_path);
    }

    #[test]
    fn resolve_falls_back_to_default_without_env_or_once() {
        assert_eq!(resolve(None, None), default_root());
    }

    #[test]
    fn default_path_finds_vendored_act_1_route() {
        // Proves the repo-root default still resolves to the real vendor
        // tree, with no env var or OnceLock override involved.
        let route = crate::vendor::vendor_dir().join("routes/act-1.txt");
        assert!(route.exists(), "expected {route:?} to exist");
    }
}
