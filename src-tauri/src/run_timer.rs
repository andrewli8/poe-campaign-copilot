//! Campaign run timer: a pausable stopwatch persisted as run_timer.json
//! next to config.json. Elapsed time is `accumulated_ms` from completed
//! running stretches plus (now - running_since_ms) while running, so the
//! frontend can tick locally from one snapshot. All transitions are pure
//! (state in, state out); IO lives in load/store/clear and is non-fatal —
//! a corrupt or unwritable file degrades to an in-memory-only timer,
//! never a crash. Mirrored by src/runTimer.ts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Guard against a hand-edited or corrupted state file ballooning; the
/// real file is a two-field JSON object of a few dozen bytes.
const MAX_STATE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunTimerState {
    /// Total milliseconds from completed (already-paused) running stretches.
    #[serde(default)]
    pub accumulated_ms: u64,
    /// Epoch-ms instant the current running stretch began; `None` while
    /// paused or never-started.
    #[serde(default)]
    pub running_since_ms: Option<u64>,
}

/// True only for the pristine state: nothing accumulated, not running.
/// Zone-entry auto-start fires ONLY from this state, so a manually paused
/// timer stays paused across zone changes.
pub fn is_never_started(state: &RunTimerState) -> bool {
    state.accumulated_ms == 0 && state.running_since_ms.is_none()
}

/// Begins a running stretch at `now_ms`. No-op if already running.
pub fn start(state: RunTimerState, now_ms: u64) -> RunTimerState {
    if state.running_since_ms.is_some() {
        return state;
    }
    RunTimerState {
        running_since_ms: Some(now_ms),
        ..state
    }
}

/// Folds the current running stretch into `accumulated_ms`. No-op if not
/// running. `saturating_sub` so a future `running_since_ms` (clock skew)
/// contributes zero rather than underflowing.
pub fn pause(state: RunTimerState, now_ms: u64) -> RunTimerState {
    let Some(since) = state.running_since_ms else {
        return state;
    };
    RunTimerState {
        accumulated_ms: state.accumulated_ms + now_ms.saturating_sub(since),
        running_since_ms: None,
    }
}

/// The hotkey action: pause if running, otherwise start/resume.
pub fn toggle(state: RunTimerState, now_ms: u64) -> RunTimerState {
    if state.running_since_ms.is_some() {
        pause(state, now_ms)
    } else {
        start(state, now_ms)
    }
}

/// Total elapsed run time at `now_ms`. Not called from the Tauri backend:
/// the frontend ticks the displayed time locally from the
/// `accumulated_ms`/`running_since_ms` snapshot broadcast on the
/// "run-timer" event, using its own mirror (`elapsedMs` in
/// src/runTimer.ts). Kept here — pure, tested, and exported alongside the
/// rest of the state machine — for parity with that mirror and any future
/// backend consumer (e.g. a tray tooltip).
#[allow(dead_code)]
pub fn elapsed_ms(state: &RunTimerState, now_ms: u64) -> u64 {
    let live = state
        .running_since_ms
        .map(|since| now_ms.saturating_sub(since))
        .unwrap_or(0);
    state.accumulated_ms + live
}

/// True for a Client.txt line that records entering a zone — the trigger
/// for auto-start. `AreaEnteredName` specifically ("You have entered X."):
/// `AreaGenerated` fires for instance creation, which can happen without
/// the player actually going in.
pub fn is_area_entered(line: &str) -> bool {
    matches!(
        event_parser::parse_line(line),
        event_parser::RawEvent::AreaEnteredName { .. }
    )
}

/// Current wall-clock time in epoch milliseconds. Pre-epoch clocks (never
/// on a real machine) degrade to 0 rather than panicking.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Resolves the state file path (`run_timer.json` next to config.json).
/// `None` if the config dir can't be resolved — callers run without
/// persistence rather than failing.
pub fn path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join("run_timer.json")),
        Err(e) => {
            crate::diagnostics::diag(&format!("run_timer: could not resolve app config dir: {e}"));
            None
        }
    }
}

/// Loads the persisted state; anything but a healthy file (missing,
/// unreadable, oversized, corrupt JSON) degrades to never-started.
pub fn load(path: &Path) -> RunTimerState {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return RunTimerState::default(), // missing: expected first-run state
    };
    if metadata.len() > MAX_STATE_BYTES {
        crate::diagnostics::diag(&format!(
            "run_timer: state file exceeds {MAX_STATE_BYTES} bytes; ignoring"
        ));
        return RunTimerState::default();
    }
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(e) => {
            crate::diagnostics::diag(&format!(
                "run_timer: failed to read {}: {e}",
                path.display()
            ));
            return RunTimerState::default();
        }
    };
    match serde_json::from_str(&json) {
        Ok(state) => state,
        Err(e) => {
            crate::diagnostics::diag(&format!(
                "run_timer: corrupt state at {}: {e}",
                path.display()
            ));
            RunTimerState::default()
        }
    }
}

/// Persists the state. Non-fatal: a failure is logged and the in-memory
/// timer keeps working for this session.
pub fn store(path: &Path, state: &RunTimerState) {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        crate::diagnostics::diag(&format!("run_timer: could not create config dir: {e}"));
        return;
    }
    match serde_json::to_string(state) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                crate::diagnostics::diag(&format!(
                    "run_timer: failed to write {}: {e}",
                    path.display()
                ));
            }
        }
        Err(e) => crate::diagnostics::diag(&format!("run_timer: failed to serialize state: {e}")),
    }
}

/// Removes the persisted state (reset / new-run). Missing file is fine.
pub fn clear(path: &Path) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        crate::diagnostics::diag(&format!(
            "run_timer: failed to remove {}: {e}",
            path.display()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_never_started() {
        let state = RunTimerState::default();
        assert!(is_never_started(&state));
        assert_eq!(elapsed_ms(&state, 1_000_000), 0);
    }

    #[test]
    fn start_begins_a_running_stretch() {
        let started = start(RunTimerState::default(), 500_000);
        assert!(!is_never_started(&started));
        assert_eq!(started.running_since_ms, Some(500_000));
        assert_eq!(elapsed_ms(&started, 510_000), 10_000);
    }

    #[test]
    fn start_on_an_already_running_timer_is_a_no_op() {
        let running = start(RunTimerState::default(), 500_000);
        assert_eq!(start(running, 600_000), running);
    }

    #[test]
    fn pause_folds_the_stretch_into_accumulated() {
        let running = start(RunTimerState::default(), 500_000);
        let paused = pause(running, 560_000);
        assert_eq!(paused.accumulated_ms, 60_000);
        assert_eq!(paused.running_since_ms, None);
        // Frozen: elapsed no longer grows with now.
        assert_eq!(elapsed_ms(&paused, 900_000), 60_000);
    }

    #[test]
    fn pause_on_a_paused_timer_is_a_no_op() {
        let paused = pause(start(RunTimerState::default(), 0), 10_000);
        assert_eq!(pause(paused, 99_000), paused);
    }

    #[test]
    fn toggle_alternates_running_and_paused() {
        let s0 = RunTimerState::default();
        let s1 = toggle(s0, 100_000); // start
        assert!(s1.running_since_ms.is_some());
        let s2 = toggle(s1, 160_000); // pause
        assert_eq!(s2.accumulated_ms, 60_000);
        assert_eq!(s2.running_since_ms, None);
        let s3 = toggle(s2, 200_000); // resume
        assert_eq!(s3.accumulated_ms, 60_000);
        assert_eq!(s3.running_since_ms, Some(200_000));
    }

    #[test]
    fn a_resumed_timer_is_not_never_started() {
        // Paused with accumulated time: zone entries must NOT auto-restart it.
        let paused = pause(start(RunTimerState::default(), 0), 10_000);
        assert!(!is_never_started(&paused));
    }

    #[test]
    fn future_running_since_clamps_to_zero_stretch() {
        let skewed = RunTimerState {
            accumulated_ms: 60_000,
            running_since_ms: Some(900_000),
        };
        assert_eq!(elapsed_ms(&skewed, 800_000), 60_000);
        // Pausing under skew must not underflow either.
        assert_eq!(pause(skewed, 800_000).accumulated_ms, 60_000);
    }

    #[test]
    fn store_load_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-run-timer-test-{}-roundtrip.json",
            std::process::id()
        ));
        let state = RunTimerState {
            accumulated_ms: 12_345,
            running_since_ms: Some(999),
        };
        store(&path, &state);
        assert_eq!(load(&path), state);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn load_missing_file_is_never_started() {
        let path = std::env::temp_dir().join("poe-copilot-run-timer-definitely-missing.json");
        assert_eq!(load(&path), RunTimerState::default());
    }

    #[test]
    fn load_corrupt_file_is_never_started() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-run-timer-test-{}-corrupt.json",
            std::process::id()
        ));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), RunTimerState::default());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn clear_removes_the_file() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-run-timer-test-{}-clear.json",
            std::process::id()
        ));
        store(&path, &RunTimerState::default());
        assert!(path.exists());
        clear(&path);
        assert!(!path.exists());
        // Clearing an already-missing file must not panic or log an error.
        clear(&path);
    }

    #[test]
    fn is_area_entered_matches_only_zone_entry_lines() {
        let enter = "2023/12/09 20:15:22 76515625 f22b6b26 [INFO Client 22384] : You have entered The Twilight Strand.";
        assert!(is_area_entered(enter));
        assert!(!is_area_entered("random chat line"));
        let level_up = "2023/12/09 20:20:00 76800000 f22b6b26 [INFO Client 22384] : Exile (Marauder) is now level 2";
        assert!(!is_area_entered(level_up));
    }
}
