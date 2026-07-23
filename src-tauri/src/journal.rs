//! Session journal: an event-sourced snapshot of campaign progress that
//! survives app restarts.
//!
//! The overlay's progress state (route cursor/step statuses, current area,
//! pinned character/level, pending tasks) lives in memory inside `Pipeline`
//! and is rebuilt from scratch on every launch. The Client.txt tailer starts
//! at end-of-file, so that state cannot be reconstructed from the game log
//! either. This module fixes that by appending every *significant* log line
//! (any line `event_parser` recognizes — area generation/entry, level-ups,
//! deaths) to a journal file next to `config.json`, and replaying that
//! journal through a fresh `Pipeline` at startup.
//!
//! Replaying the journal uses the exact same `Pipeline::on_line` path as
//! live tailing, so restored state is consistent with live semantics by
//! construction. A corrupt, truncated, or oversized journal degrades to a
//! fresh session (never a crash): unparseable lines are `RawEvent::Unknown`
//! no-ops, and an unreadable/oversized file is treated as absent.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::pipeline::Pipeline;

/// Hard cap on the journal file we'll read back. A full campaign produces
/// on the order of a few thousand significant lines (~hundreds of KB), so
/// anything near this size is corrupt or runaway rather than a legitimate
/// journal; `load_lines` degrades to an empty (fresh-session) journal
/// instead of buffering an unbounded file into memory.
pub const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;

/// True for any Client.txt line the event parser recognizes as a session
/// event (area generated/entered, level-up, slain). Only these lines are
/// worth journaling: everything else is a `RawEvent::Unknown` no-op that
/// would bloat the journal without affecting replayed state.
pub fn is_significant(line: &str) -> bool {
    !matches!(
        event_parser::parse_line(line),
        event_parser::RawEvent::Unknown { .. }
    )
}

/// Resolves the journal file path (`session_journal.log` in the app config
/// dir, next to `config.json`). `None` if the config dir can't be resolved
/// — callers then simply run without persistence rather than failing.
pub fn journal_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    match app.path().app_config_dir() {
        Ok(dir) => Some(dir.join("session_journal.log")),
        Err(e) => {
            eprintln!("journal: could not resolve app config dir: {e}");
            None
        }
    }
}

/// Reads the journal back as a list of lines, oldest first. Degrades to an
/// empty list (fresh session) on every failure mode: missing file (the
/// normal first-run case, silent), unreadable file, or a file over `cap`
/// bytes (logged). Blank lines are dropped; corrupt lines are kept — they
/// replay as harmless `Unknown` no-ops.
pub fn load_lines(path: &Path, cap: u64) -> Vec<String> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            eprintln!("journal: failed to open {}: {e}", path.display());
            return Vec::new();
        }
    };
    match file.metadata() {
        Ok(m) if m.len() > cap => {
            eprintln!(
                "journal: {} exceeds {cap} byte cap; starting a fresh session",
                path.display()
            );
            return Vec::new();
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("journal: failed to stat {}: {e}", path.display());
            return Vec::new();
        }
    }
    let text = match std::io::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("journal: failed to read {}: {e}", path.display());
            return Vec::new();
        }
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// Appends one line to the journal, creating the file (and its parent
/// directory) if needed. Single-line `O_APPEND` writes are effectively
/// atomic at the sizes involved; a torn write from a crash produces at
/// worst one corrupt line, which replays as an `Unknown` no-op.
pub fn append_line(path: &Path, line: &str) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Err(format!("could not create journal dir: {e}"));
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("could not open journal: {e}"))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| format!("could not append to journal: {e}"))
}

/// Removes the journal file (used when settings change and the pipeline is
/// rebuilt fresh — a stale journal would resurrect the discarded progress
/// on the next launch). A missing file is success, not an error.
pub fn clear(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("could not remove journal: {e}")),
    }
}

/// Feeds journaled lines through the pipeline exactly as live tailing
/// would, discarding the intermediate UI models. Returns the number of
/// lines fed (for startup logging).
pub fn replay_into<'a>(
    pipeline: &mut Pipeline,
    lines: impl IntoIterator<Item = &'a String>,
) -> usize {
    let mut count = 0usize;
    for line in lines {
        let _ = pipeline.on_line(line);
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::compile::Variant;

    const GEN_STRAND: &str = r#"2026/07/21 19:00:01 1000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_1" with seed 101"#;
    const ENTER_STRAND: &str = "2026/07/21 19:00:03 3000 f22b6b26 [INFO Client 900] : You have entered The Twilight Strand.";
    const LEVEL_UP: &str = "2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2";
    const GEN_TOWN: &str = r#"2026/07/21 19:03:20 200000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_town" with seed 555"#;
    const ENTER_TOWN: &str =
        "2026/07/21 19:03:22 202000 f22b6b26 [INFO Client 900] : You have entered Lioneye's Watch.";
    const GEN_COAST: &str = r#"2026/07/21 19:04:00 240000 1186a0e0 [DEBUG Client 900] Generating level 2 area "1_1_2" with seed 8801"#;
    const ENTER_COAST: &str =
        "2026/07/21 19:04:02 242000 f22b6b26 [INFO Client 900] : You have entered The Coast.";

    fn temp_journal(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "poe-copilot-journal-test-{}-{tag}.log",
            std::process::id()
        ))
    }

    #[test]
    fn is_significant_recognizes_session_events_only() {
        assert!(is_significant(GEN_STRAND));
        assert!(is_significant(ENTER_STRAND));
        assert!(is_significant(LEVEL_UP));
        assert!(!is_significant("total garbage"));
        assert!(!is_significant(
            "2026/07/21 19:05:00 300000 abc [INFO Client 900] #global chat noise"
        ));
        assert!(!is_significant(""));
    }

    #[test]
    fn load_lines_missing_file_is_empty() {
        let path = temp_journal("missing");
        let _ = std::fs::remove_file(&path);
        assert!(load_lines(&path, MAX_JOURNAL_BYTES).is_empty());
    }

    #[test]
    fn append_then_load_round_trips_in_order() {
        let path = temp_journal("roundtrip");
        let _ = std::fs::remove_file(&path);
        append_line(&path, GEN_STRAND).unwrap();
        append_line(&path, ENTER_STRAND).unwrap();
        append_line(&path, LEVEL_UP).unwrap();
        let lines = load_lines(&path, MAX_JOURNAL_BYTES);
        assert_eq!(lines, vec![GEN_STRAND, ENTER_STRAND, LEVEL_UP]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn append_line_creates_missing_parent_directory() {
        let dir = std::env::temp_dir().join(format!(
            "poe-copilot-journal-test-{}-nested",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("deep").join("session_journal.log");
        append_line(&path, ENTER_STRAND).unwrap();
        assert_eq!(load_lines(&path, MAX_JOURNAL_BYTES), vec![ENTER_STRAND]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn oversized_journal_degrades_to_empty() {
        let path = temp_journal("oversized");
        std::fs::write(&path, "x".repeat(200)).unwrap();
        assert!(load_lines(&path, 100).is_empty());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn clear_removes_file_and_is_idempotent() {
        let path = temp_journal("clear");
        append_line(&path, ENTER_STRAND).unwrap();
        clear(&path).unwrap();
        assert!(!path.exists());
        // Clearing an already-missing journal is success, not an error.
        clear(&path).unwrap();
    }

    #[test]
    fn replay_into_restores_route_position() {
        // Simulates a relaunch: a fresh Pipeline (as built at startup) fed
        // the journal from the previous session must come back NOT waiting
        // for the log, positioned at the last-entered zone.
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        assert!(p.current_model().waiting_for_log, "fresh pipeline waits");

        let journal: Vec<String> = [
            GEN_STRAND,
            ENTER_STRAND,
            LEVEL_UP,
            GEN_TOWN,
            ENTER_TOWN,
            GEN_COAST,
            ENTER_COAST,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let fed = replay_into(&mut p, &journal);
        assert_eq!(fed, journal.len());

        let m = p.current_model();
        assert!(!m.waiting_for_log, "restored session must not re-wait");
        assert_eq!(m.overlay.zone_name, "The Coast");
    }

    #[test]
    fn replay_into_survives_garbage_lines() {
        // A torn write or hand-edited journal must degrade per-line, never
        // panic or poison the rest of the replay.
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        let journal: Vec<String> = [
            "not a log line at all",
            GEN_STRAND,
            "2026/07/21 19:00:02 2000 truncated garba",
            ENTER_STRAND,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        replay_into(&mut p, &journal);
        let m = p.current_model();
        assert!(!m.waiting_for_log);
        assert_eq!(m.overlay.zone_name, "The Twilight Strand");
    }

    #[test]
    fn full_persist_and_restore_cycle() {
        // End-to-end contract for the restart bug: session 1 appends only
        // its significant lines; session 2 loads + replays and resumes at
        // the same route position.
        let path = temp_journal("cycle");
        let _ = std::fs::remove_file(&path);

        // Session 1: live lines arrive, significant ones get journaled.
        for line in [
            GEN_STRAND,
            ENTER_STRAND,
            "chat/system noise that must not be journaled",
            GEN_COAST,
            ENTER_COAST,
        ] {
            if is_significant(line) {
                append_line(&path, line).unwrap();
            }
        }

        // Session 2 (relaunch): fresh pipeline + journal replay.
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        let lines = load_lines(&path, MAX_JOURNAL_BYTES);
        assert_eq!(lines.len(), 4, "only significant lines are persisted");
        replay_into(&mut p, &lines);
        let m = p.current_model();
        assert!(!m.waiting_for_log);
        assert_eq!(m.overlay.zone_name, "The Coast");

        std::fs::remove_file(&path).unwrap();
    }
}
