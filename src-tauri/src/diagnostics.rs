//! User-retrievable diagnostics logging.
//!
//! Installed users double-click the app and never see a terminal, so
//! `eprintln!` output (the app's only diagnostics before this module)
//! simply vanishes for them. This module mirrors user-relevant `eprintln!`
//! calls into a small rotating log file under the OS app-log directory, so
//! a user can grab that file for a bug report, and installs a panic hook
//! so a crash leaves a trace even when nobody was watching a terminal.
//!
//! The log path is resolved once, early in `.setup()` (see `init`), and
//! cached in a process-lifetime `OnceLock`. `write`/`diag` calls before
//! `init` runs (or if it failed to resolve a path) are stderr-only —
//! there's no file to append to yet.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;

use tauri::Manager;

/// Log file name, resolved under `app.path().app_log_dir()`.
const LOG_FILE_NAME: &str = "poe-copilot.log";

/// Simple size cap for the log file. There's no rotation-with-history here
/// (see `should_rotate`/`write`) — once the file exceeds this, it's simply
/// recreated empty. Good enough for a diagnostics log whose job is "what
/// just happened", not a long-term audit trail.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB

/// Resolved once at startup by `init`; every `write`/`diag` call reads this
/// rather than re-resolving the app handle's log dir on every call.
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Resolves (creating if needed) the diagnostics log file path:
/// `<app_log_dir>/poe-copilot.log`. `None` if the log directory can't be
/// resolved or created — callers then simply run without file logging
/// (stderr keeps working via `diag`).
///
/// Exposed (not just used internally by `init`) so the "Open logs" tray
/// item can re-resolve the same path without needing the cached
/// `OnceLock` value threaded through the menu-event closure.
pub fn log_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = match app.path().app_log_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("diagnostics: could not resolve app log dir: {e}");
            return None;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "diagnostics: could not create log dir {}: {e}",
            dir.display()
        );
        return None;
    }
    Some(dir.join(LOG_FILE_NAME))
}

/// Resolves and caches the log file path for the rest of the process's
/// lifetime. MUST be called first thing in `.setup()`, before any other
/// setup step that might log — everything that logs before this runs only
/// reaches stderr, not the file. Safe to call more than once (the first
/// successfully resolved path wins; later calls are no-ops), though the
/// app only ever calls this once.
pub fn init(app: &tauri::AppHandle) {
    if let Some(path) = log_path(app) {
        let _ = LOG_PATH.set(path);
    }
}

/// True when a log file of `len` bytes should be rotated (recreated empty)
/// before the next write. A pure predicate, kept separate from `write` so
/// it's unit-testable without touching the filesystem.
fn should_rotate(len: u64) -> bool {
    len > MAX_LOG_BYTES
}

/// Current time as seconds since the Unix epoch. Used instead of a
/// human-readable ISO-8601 timestamp to avoid pulling in a `chrono`
/// dependency for this one call site; an epoch stamp still lets a bug
/// report be correlated against "when did this happen" (any epoch
/// converter, or `date -d @<secs>` on Unix, turns it back into a
/// calendar date). Pre-epoch clocks (never on a real machine) degrade to
/// 0 rather than panicking.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Formats one diagnostics log line: `<epoch-seconds> <msg>\n`. Pure and
/// unit-tested separately from the IO in `write`.
fn format_line(ts_secs: u64, msg: &str) -> String {
    format!("{ts_secs} {msg}\n")
}

/// Appends one line to the diagnostics log file, if `init` has resolved a
/// path (a no-op otherwise — e.g. a panic before `.setup()` runs). Opens,
/// appends, and closes the file per call rather than holding it open:
/// diagnostics writes are infrequent enough that the extra open/close cost
/// doesn't matter, and not holding a handle means nothing here can leak a
/// file descriptor across the process's lifetime.
///
/// Rotates (truncates to empty) first if the file has grown past
/// `MAX_LOG_BYTES`, so a long-running session can't let this file grow
/// without bound.
///
/// Never panics on IO failure — swallowed intentionally. A diagnostics
/// write failing (e.g. a read-only/full disk) must not itself crash the
/// app it exists to leave a trace for, and there's nowhere better to
/// surface the failure: stderr goes nowhere for the installed, no-terminal
/// user this module exists to serve.
pub fn write(msg: &str) {
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    if let Ok(meta) = std::fs::metadata(path)
        && should_rotate(meta.len())
    {
        let _ = std::fs::write(path, b"");
    }
    let line = format_line(now_secs(), msg);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Writes `msg` to BOTH stderr (`eprintln!`, kept so `npm run tauri dev`
/// still shows it live) and the diagnostics log file (`write`, the part
/// that actually reaches an installed, no-terminal user). This is the
/// call every user-relevant diagnostic in the app should go through
/// instead of a bare `eprintln!`.
pub fn diag(msg: &str) {
    eprintln!("{msg}");
    write(msg);
}

/// Reads the newest `max_bytes` of the log at `path` as complete lines
/// (the first, possibly byte-torn line of an over-cap read is dropped —
/// same trick as `journal::load_lines`). `None` when the file is missing,
/// unreadable, or has no non-blank content — callers (the bug-report
/// prefill) then fall back to behaving as if there were no log at all.
/// Read-only and side-effect-free: reporting a bug must never itself
/// write to the log it is bundling.
pub fn tail(path: &std::path::Path, max_bytes: u64) -> Option<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let over_cap = len > max_bytes;
    if over_cap {
        file.seek(SeekFrom::End(-(max_bytes as i64))).ok()?;
    }
    let mut buf = Vec::with_capacity(len.min(max_bytes) as usize);
    file.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let text = if over_cap {
        match text.find('\n') {
            Some(i) => &text[i + 1..],
            None => "", // one giant torn line: nothing complete to keep
        }
    } else {
        &text[..]
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Installs a panic hook that leaves a trace in the diagnostics log before
/// chaining to the default hook (which still prints to stderr, preserving
/// today's `tauri dev` behavior). This is the highest-value part of this
/// module: for an installed user, a crash with no terminal open would
/// otherwise leave nothing at all to put in a bug report.
///
/// Must be installed before `tauri::Builder` runs, so it's active for the
/// whole process lifetime, including any panic during Tauri's own setup.
/// At that point `init` may not have run yet (the log path isn't resolved
/// until inside `.setup()`), so `write` is called directly rather than
/// `diag` — `write` already no-ops gracefully when `LOG_PATH` is unset,
/// which is exactly the guard a very-early panic needs.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        write(&format!("PANIC at {location}: {payload}"));
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_rotate_false_at_and_below_the_cap() {
        assert!(!should_rotate(0));
        assert!(!should_rotate(MAX_LOG_BYTES));
    }

    #[test]
    fn should_rotate_true_above_the_cap() {
        assert!(should_rotate(MAX_LOG_BYTES + 1));
    }

    #[test]
    fn format_line_has_timestamp_message_and_trailing_newline() {
        assert_eq!(format_line(12_345, "hello world"), "12345 hello world\n");
    }

    #[test]
    fn format_line_handles_an_empty_message() {
        assert_eq!(format_line(0, ""), "0 \n");
    }

    #[test]
    fn write_before_init_is_a_harmless_noop() {
        // LOG_PATH is a process-wide OnceLock that other tests in this
        // binary may or may not have set (test execution order/threading
        // is not under our control), so this doesn't assert on LOG_PATH's
        // state directly — it only asserts that calling `write`/`diag`
        // never panics regardless of whether a path has been resolved.
        write("pre-init message");
        diag("pre-init message via diag");
    }

    fn temp_log(name: &str, content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-diag-tail-test-{}-{name}.log",
            std::process::id()
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn tail_returns_a_small_file_whole() {
        let path = temp_log("whole", "1 first\n2 second\n");
        assert_eq!(tail(&path, 1_024).as_deref(), Some("1 first\n2 second"));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn tail_of_an_over_cap_file_keeps_only_newest_complete_lines() {
        let content: String = (0..100)
            .map(|i| format!("{i:03} event happened\n"))
            .collect();
        let path = temp_log("overcap", &content);
        let got = tail(&path, 100).expect("tail yields content");
        // Newest line survives; the byte-torn first line was dropped, so
        // every returned line is a complete original line.
        assert!(got.ends_with("099 event happened"));
        for line in got.lines() {
            assert!(line.ends_with("event happened"), "torn line kept: {line:?}");
        }
        assert!(!got.contains("000 event happened"));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn tail_is_none_for_missing_or_blank_files() {
        let missing = std::env::temp_dir().join("poe-copilot-diag-tail-definitely-missing.log");
        assert_eq!(tail(&missing, 1_024), None);

        let blank = temp_log("blank", "  \n\n  \n");
        assert_eq!(tail(&blank, 1_024), None);
        std::fs::remove_file(&blank).unwrap();
    }
}
