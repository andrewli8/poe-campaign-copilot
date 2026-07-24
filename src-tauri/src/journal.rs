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

/// Hard cap on how much of the journal file we'll ever buffer into memory.
/// A full campaign produces on the order of a few thousand significant
/// lines (~hundreds of KB), so a file past this size is runaway growth
/// (e.g. the app left tailing for a whole league). `load_lines` never
/// degrades an over-cap file to nothing — that would silently and
/// PERMANENTLY disable resume once the cap was ever crossed, since the
/// append-only file would keep growing. Instead it loads the newest
/// cap-sized tail (see `load_lines`), and startup compaction (`compact`)
/// then shrinks the file back under `COMPACT_TARGET_BYTES`.
pub const MAX_JOURNAL_BYTES: u64 = 8 * 1024 * 1024;

/// Startup compaction target. Deliberately far below `MAX_JOURNAL_BYTES`:
/// after compaction the file has this much room to grow again before the
/// read cap even becomes relevant, so in steady state every launch replays
/// the complete journal and the tail-truncation path is a last resort.
pub const COMPACT_TARGET_BYTES: u64 = 2 * 1024 * 1024;

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
            crate::diagnostics::diag(&format!("journal: could not resolve app config dir: {e}"));
            None
        }
    }
}

/// Reads the journal back as a list of lines, oldest first, buffering at
/// most `cap` bytes. A file within the cap is read whole. A file OVER the
/// cap still restores: only its newest `cap`-byte tail is read, and the
/// first (byte-offset-torn, partial) line of that tail is dropped so every
/// returned line is a complete original line. Degrades to an empty list
/// (fresh session) on the true failure modes: missing file (the normal
/// first-run case, silent) or an unreadable file (logged). Blank lines are
/// dropped; corrupt lines are kept — they replay as harmless `Unknown`
/// no-ops.
pub fn load_lines(path: &Path, cap: u64) -> Vec<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            crate::diagnostics::diag(&format!("journal: failed to open {}: {e}", path.display()));
            return Vec::new();
        }
    };
    let len = match file.metadata() {
        Ok(m) => m.len(),
        Err(e) => {
            crate::diagnostics::diag(&format!("journal: failed to stat {}: {e}", path.display()));
            return Vec::new();
        }
    };
    let over_cap = len > cap;
    if over_cap {
        crate::diagnostics::diag(&format!(
            "journal: {} is {len} bytes (over the {cap} byte cap); \
             restoring from the newest {cap}-byte tail only",
            path.display()
        ));
        if let Err(e) = file.seek(SeekFrom::End(-(cap as i64))) {
            crate::diagnostics::diag(&format!("journal: failed to seek {}: {e}", path.display()));
            return Vec::new();
        }
    }
    let mut buf = Vec::with_capacity(len.min(cap) as usize);
    if let Err(e) = file.read_to_end(&mut buf) {
        crate::diagnostics::diag(&format!("journal: failed to read {}: {e}", path.display()));
        return Vec::new();
    }
    // The seek above may have landed mid-line (and, for that matter,
    // mid-UTF-8-sequence); a lossy decode plus dropping everything up to
    // the first newline yields only complete original lines.
    let text = String::from_utf8_lossy(&buf);
    let text = if over_cap {
        match text.find('\n') {
            Some(i) => &text[i + 1..],
            None => "", // one giant torn line: nothing complete to keep
        }
    } else {
        &text[..]
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// Shrinks the journal file back under `target` bytes by rewriting it to
/// the longest suffix of `lines` (the lines just loaded/replayed — newest
/// last) that fits. No-op (`Ok(false)`) when the file is already within
/// `target`. Crash-safe: the suffix is written to a sibling temp file and
/// atomically renamed over the journal, so an interrupted compaction
/// leaves the original journal untouched (a leftover `.tmp` is simply
/// overwritten next time).
///
/// Called on startup after replay. Trimming oldest-first degrades
/// gracefully rather than breaking resume: recent area/level lines carry
/// almost all restorable state, and the route engine fast-forwards from a
/// suffix on replay.
pub fn compact(path: &Path, lines: &[String], target: u64) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Ok(m) if m.len() <= target => return Ok(false),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("could not stat journal: {e}")),
    }

    // Longest suffix of `lines` whose serialized size (line + '\n' each)
    // fits in `target`.
    let mut start = lines.len();
    let mut total: u64 = 0;
    while start > 0 {
        let next = lines[start - 1].len() as u64 + 1;
        if total + next > target {
            break;
        }
        total += next;
        start -= 1;
    }

    let tmp = path.with_extension("log.tmp");
    let mut out = String::with_capacity(total as usize);
    for line in &lines[start..] {
        out.push_str(line);
        out.push('\n');
    }
    std::fs::write(&tmp, &out).map_err(|e| format!("could not write journal temp file: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("could not replace journal: {e}"))?;
    Ok(true)
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
    const LEVEL_UP: &str =
        "2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2";
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
    fn oversized_journal_loads_newest_tail_at_a_line_boundary() {
        // An over-cap journal must NOT degrade to empty (that would
        // silently and permanently disable resume once the cap is ever
        // crossed). It loads the newest lines that fit, dropping the
        // partial line the byte-offset cut lands in.
        let path = temp_journal("oversized-tail");
        let all: Vec<String> = (0..40).map(|i| format!("line-number-{i:04}")).collect();
        std::fs::write(&path, all.join("\n") + "\n").unwrap();

        let cap = 100; // far smaller than the file
        let lines = load_lines(&path, cap);
        assert!(
            !lines.is_empty(),
            "oversized journal must still load a tail"
        );
        // Every returned line is a complete original line (no torn line
        // from cutting at an arbitrary byte offset)...
        for l in &lines {
            assert!(all.contains(l), "torn/partial line returned: {l:?}");
        }
        // ...and they are exactly the newest suffix, in order.
        assert_eq!(lines.last().unwrap(), "line-number-0039");
        let suffix = &all[all.len() - lines.len()..];
        assert_eq!(lines, suffix);
        // The kept tail respects the byte cap.
        let total: usize = lines.iter().map(|l| l.len() + 1).sum();
        assert!(total as u64 <= cap);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn oversized_single_line_journal_degrades_to_empty() {
        // Pathological: one giant line with no newline inside the tail
        // window — nothing complete to keep, degrade to fresh (not crash).
        let path = temp_journal("oversized-one-line");
        std::fs::write(&path, "x".repeat(200)).unwrap();
        assert!(load_lines(&path, 100).is_empty());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn compact_is_a_noop_when_under_target() {
        let path = temp_journal("compact-noop");
        let _ = std::fs::remove_file(&path);
        append_line(&path, ENTER_STRAND).unwrap();
        let lines = load_lines(&path, MAX_JOURNAL_BYTES);
        let rewritten = compact(&path, &lines, MAX_JOURNAL_BYTES).unwrap();
        assert!(!rewritten);
        assert_eq!(load_lines(&path, MAX_JOURNAL_BYTES), vec![ENTER_STRAND]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn compact_shrinks_an_oversized_journal_to_the_newest_suffix() {
        let path = temp_journal("compact-shrink");
        let all: Vec<String> = (0..40).map(|i| format!("line-number-{i:04}")).collect();
        std::fs::write(&path, all.join("\n") + "\n").unwrap();

        let target = 120;
        let lines = load_lines(&path, MAX_JOURNAL_BYTES);
        let rewritten = compact(&path, &lines, target).unwrap();
        assert!(rewritten);
        assert!(std::fs::metadata(&path).unwrap().len() <= target);

        let kept = load_lines(&path, MAX_JOURNAL_BYTES);
        assert!(!kept.is_empty());
        assert_eq!(kept.last().unwrap(), "line-number-0039");
        assert_eq!(kept, &all[all.len() - kept.len()..]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn journal_grown_past_cap_still_restores_and_is_shrunk_on_next_launch() {
        // The coordinator's scenario: the journal grows past the read cap.
        // The next launch must (a) still restore the session from the
        // newest tail and (b) compact the file back under target so it
        // can't ratchet toward permanent breakage.
        let path = temp_journal("cap-recovery");
        let _ = std::fs::remove_file(&path);

        // Old history: filler significant-shaped noise (unknown-but-parsed
        // garbage is fine — replay treats it as no-ops) pushing well past
        // the cap, with the real recent session at the very end.
        let filler = format!(
            "2026/07/20 10:00:00 1 abc [INFO Client 900] old noise {}",
            "y".repeat(80)
        );
        let mut content = String::new();
        for _ in 0..40 {
            content.push_str(&filler);
            content.push('\n');
        }
        for line in [GEN_STRAND, ENTER_STRAND, GEN_COAST, ENTER_COAST] {
            content.push_str(line);
            content.push('\n');
        }
        std::fs::write(&path, &content).unwrap();

        let cap = 1200; // smaller than the file; big enough for the real tail
        let target = 800;
        assert!(std::fs::metadata(&path).unwrap().len() > cap);

        // Launch: load tail, replay, compact.
        let lines = load_lines(&path, cap);
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        replay_into(&mut p, &lines);
        let m = p.current_model();
        assert!(!m.waiting_for_log, "tail restore must still work past cap");
        assert_eq!(m.overlay.zone_name, "The Coast");
        assert!(compact(&path, &lines, target).unwrap());
        assert!(std::fs::metadata(&path).unwrap().len() <= target);

        // Next launch after compaction: still restores.
        let lines = load_lines(&path, cap);
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        replay_into(&mut p, &lines);
        let m = p.current_model();
        assert!(!m.waiting_for_log);
        assert_eq!(m.overlay.zone_name, "The Coast");

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
    fn reset_returns_pipeline_to_a_fresh_waiting_state() {
        // The manual "Reset progress" action drops all live session state
        // back to a fresh start, keeping config-derived data (route, build,
        // layouts). After it, the overlay waits for the new character's
        // first zone and the route is no longer complete.
        let mut p = Pipeline::new(Variant::LeagueStart, None).unwrap();
        let journal: Vec<String> = [GEN_STRAND, ENTER_STRAND, LEVEL_UP]
            .iter()
            .map(|s| s.to_string())
            .collect();
        replay_into(&mut p, &journal);
        assert!(
            !p.current_model().waiting_for_log,
            "pipeline made progress before reset"
        );

        p.reset();

        let m = p.current_model();
        assert!(
            m.waiting_for_log,
            "reset returns to waiting for the first zone"
        );
        assert!(!m.overlay.route_complete, "reset clears route completion");
        assert_eq!(m.overlay.act, 1, "route is back at act 1");
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
