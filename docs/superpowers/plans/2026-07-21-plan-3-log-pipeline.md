# Plan 3: Client.txt Log Pipeline (tailer, parser, session, replay)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the passive game-state pipeline: resilient Client.txt tailing, typed event parsing, session tracking (current area/act/town, new-instance detection), a deterministic replay harness with a golden fixture, and a `fake-play` binary that simulates live play on macOS.

**Architecture:** Three new library crates mirroring the design spec's module boundaries — `input_log` (bytes → lines, file polling resilient to truncation/replacement), `event_parser` (line → typed `RawEvent`, unknown lines fingerprinted, never an error), `session` (RawEvents → high-level `SessionEvent`s using the vendored area map; area identity comes from `Generating level N area "<id>"` debug lines, with English "You have entered X." lines as confirmation) — plus a `replay` crate (fixture-driven harness + `fake-play` bin). All std-only except serde on event types.

**Tech Stack:** Rust (std, serde), existing `content` crate for the area map. No regex, no chrono, no notify, no tempfile — std does all of it.

## Global Constraints

- Passive-only: read files, never write to the game's file, no network, no process APIs.
- English clients only: the parser matches English message text; area IDs (not names) are the source of truth wherever available.
- New crates are std-only plus `serde` (derive) where types cross crate boundaries; `content` dependency allowed in `session` and `replay`. No other deps.
- Unknown log lines are DATA (`RawEvent::Unknown` with a digit-stripped fingerprint), never parse errors. The pipeline must survive arbitrary garbage input.
- Client.txt line shape (English, stable for years):
  - `2023/12/09 20:15:19 76512546 1186a0e0 [DEBUG Client 22384] Generating level 1 area "1_1_1" with seed 1780194933`
  - `2023/12/09 20:15:22 76515625 f22b6b26 [INFO Client 22384] : You have entered The Twilight Strand.`
  - `2023/12/09 20:18:40 76713640 f22b6b26 [INFO Client 22384] : Wanderer (Ranger) is now level 2` (no trailing period)
  - `2023/12/09 20:20:01 76794001 f22b6b26 [INFO Client 22384] : Wanderer has been slain.`
  - Timestamp = first 19 chars. Message begins after `"] "`; INFO gameplay messages additionally start with `": "`.
- Edition 2024, workspace conventions, commit format `<type>: <description>`, no AI attribution. Controller pushes after each task.
- These log formats are from documented community knowledge; the Windows validation session against a real Client.txt is the final arbiter. If a fixture-vs-reality mismatch is found later, fixtures change to match reality — parser semantics never get "corrected" against invented data (Plan 2's fabrication lesson).

---

### Task 1: `input_log` crate — line assembly and resilient file polling

**Files:**
- Create: `crates/input_log/Cargo.toml`, `crates/input_log/src/lib.rs`, `crates/input_log/src/assembler.rs`, `crates/input_log/src/poller.rs`, `crates/input_log/src/tailer.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces:
  - `input_log::LineAssembler` — `pub fn new() -> Self` (also `Default`), `pub fn feed(&mut self, chunk: &[u8]) -> Vec<String>` — splits on `\n`, strips `\r`, holds the partial tail across calls, lossy UTF-8.
  - `input_log::FilePoller` — `pub fn new(path: PathBuf, start_at_end: bool) -> std::io::Result<Self>`, `pub fn poll(&mut self) -> std::io::Result<Vec<String>>`. Semantics: missing file → `Ok(vec![])`; file length < stored offset (truncation or replacement) → reset offset to 0 and clear the assembler, then read from start; otherwise read `[offset, len)`, advance offset, return complete lines.
  - `input_log::spawn_tailer(poller: FilePoller, poll_interval: std::time::Duration, sender: std::sync::mpsc::Sender<String>) -> TailerHandle` — background thread; `TailerHandle::stop(self)` sets an `AtomicBool` and joins. Send errors (receiver dropped) end the thread.

- [ ] **Step 1: Crate scaffold**

`crates/input_log/Cargo.toml`:

```toml
[package]
name = "input_log"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
```

`crates/input_log/src/lib.rs`:

```rust
//! Passive Client.txt tailing: byte chunks -> complete text lines,
//! resilient to partial writes, truncation, and file replacement.

mod assembler;
mod poller;
mod tailer;

pub use assembler::LineAssembler;
pub use poller::FilePoller;
pub use tailer::{spawn_tailer, TailerHandle};
```

Add `"crates/input_log"` to the workspace `members` array in the root `Cargo.toml`.

- [ ] **Step 2: Write failing assembler tests**

`crates/input_log/src/assembler.rs` — tests first with a stub:

```rust
#[derive(Default)]
pub struct LineAssembler {
    partial: Vec<u8>,
}

impl LineAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, _chunk: &[u8]) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_complete_lines_and_holds_partial_tail() {
        let mut a = LineAssembler::new();
        assert_eq!(a.feed(b"hello\nwor"), vec!["hello".to_string()]);
        assert_eq!(a.feed(b"ld\n"), vec!["world".to_string()]);
        assert_eq!(a.feed(b""), Vec::<String>::new());
    }

    #[test]
    fn strips_carriage_returns() {
        let mut a = LineAssembler::new();
        assert_eq!(a.feed(b"a\r\nb\r\n"), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn multiple_lines_in_one_chunk_and_split_across_chunks() {
        let mut a = LineAssembler::new();
        assert_eq!(
            a.feed(b"one\ntwo\nthr"),
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(a.feed(b"ee\nfour\n"), vec!["three".to_string(), "four".to_string()]);
    }

    #[test]
    fn invalid_utf8_is_lossy_not_fatal() {
        let mut a = LineAssembler::new();
        let lines = a.feed(b"ok\xFF\xFEline\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("ok"));
        assert!(lines[0].ends_with("line"));
    }
}
```

Run: `cargo test -p input_log` — expect the first three tests FAIL (stub returns empty).

- [ ] **Step 3: Implement the assembler**

Replace `feed`:

```rust
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.partial.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.partial.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.partial.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        lines
    }
```

Run: `cargo test -p input_log` — assembler tests PASS.

- [ ] **Step 4: Write failing poller tests**

`crates/input_log/src/poller.rs` — tests use a unique temp file (std only):

```rust
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::assembler::LineAssembler;

pub struct FilePoller {
    path: PathBuf,
    offset: u64,
    assembler: LineAssembler,
}

impl FilePoller {
    pub fn new(path: PathBuf, start_at_end: bool) -> std::io::Result<Self> {
        let offset = if start_at_end {
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        Ok(Self { path, offset, assembler: LineAssembler::new() })
    }

    pub fn poll(&mut self) -> std::io::Result<Vec<String>> {
        Ok(Vec::new()) // stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "poe-copilot-test-{}-{n}.log",
            std::process::id()
        ))
    }

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn reads_appended_lines_across_polls() {
        let path = temp_path();
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "first\n").unwrap();
        let mut p = FilePoller::new(path.clone(), false).unwrap();
        assert_eq!(p.poll().unwrap(), vec!["first".to_string()]);
        assert_eq!(p.poll().unwrap(), Vec::<String>::new());

        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(f, "sec").unwrap();
        assert_eq!(p.poll().unwrap(), Vec::<String>::new()); // partial line held
        writeln!(f, "ond").unwrap();
        assert_eq!(p.poll().unwrap(), vec!["second".to_string()]);
    }

    #[test]
    fn start_at_end_skips_existing_content() {
        let path = temp_path();
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "old1\nold2\n").unwrap();
        let mut p = FilePoller::new(path.clone(), true).unwrap();
        assert_eq!(p.poll().unwrap(), Vec::<String>::new());
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "new").unwrap();
        assert_eq!(p.poll().unwrap(), vec!["new".to_string()]);
    }

    #[test]
    fn truncation_resets_to_start() {
        let path = temp_path();
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "aaaa\nbbbb\n").unwrap();
        let mut p = FilePoller::new(path.clone(), false).unwrap();
        p.poll().unwrap();
        std::fs::write(&path, "cc\n").unwrap(); // shorter: truncation/replacement
        assert_eq!(p.poll().unwrap(), vec!["cc".to_string()]);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = temp_path(); // never created
        let mut p = FilePoller::new(path, false).unwrap();
        assert_eq!(p.poll().unwrap(), Vec::<String>::new());
    }
}
```

Run: `cargo test -p input_log poller` — expect FAIL.

- [ ] **Step 5: Implement `poll`**

```rust
    pub fn poll(&mut self) -> std::io::Result<Vec<String>> {
        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let len = file.metadata()?.len();
        if len < self.offset {
            // Truncated or replaced: start over. The partial tail belongs to
            // the old file and is discarded.
            self.offset = 0;
            self.assembler = LineAssembler::new();
        }
        if len == self.offset {
            return Ok(Vec::new());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::with_capacity((len - self.offset) as usize);
        file.take(len - self.offset).read_to_end(&mut buf)?;
        self.offset = len;
        Ok(self.assembler.feed(&buf))
    }
```

Run: `cargo test -p input_log` — all PASS.

- [ ] **Step 6: Tailer thread wrapper**

`crates/input_log/src/tailer.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::poller::FilePoller;

pub struct TailerHandle {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

impl TailerHandle {
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.join.join();
    }
}

pub fn spawn_tailer(
    mut poller: FilePoller,
    poll_interval: Duration,
    sender: Sender<String>,
) -> TailerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let join = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::SeqCst) {
            match poller.poll() {
                Ok(lines) => {
                    for line in lines {
                        if sender.send(line).is_err() {
                            return; // receiver gone
                        }
                    }
                }
                Err(_) => { /* transient I/O error: retry next tick */ }
            }
            std::thread::sleep(poll_interval);
        }
    });
    TailerHandle { stop, join }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tails_appends_until_stopped() {
        let path = std::env::temp_dir().join(format!(
            "poe-copilot-tailer-{}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "").unwrap();

        let poller = FilePoller::new(path.clone(), false).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = spawn_tailer(poller, Duration::from_millis(10), tx);

        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "alpha").unwrap();
        writeln!(f, "beta").unwrap();
        f.flush().unwrap();

        let a = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let b = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("alpha", "beta"));

        handle.stop();
        let _ = std::fs::remove_file(&path);
    }
}
```

Run: `cargo test -p input_log` — all PASS. Then `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/input_log/
git commit -m "feat: input_log crate with resilient Client.txt tailing"
```

---

### Task 2: `event_parser` crate — typed events from log lines

**Files:**
- Create: `crates/event_parser/Cargo.toml`, `crates/event_parser/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces:
  - `event_parser::RawEvent` — `#[derive(Debug, Clone, PartialEq, Serialize)]`, variants:
    - `AreaGenerated { area_id: String, area_level: u8, seed: u64, at: String }`
    - `AreaEnteredName { display_name: String, at: String }`
    - `LevelUp { character: String, class: String, level: u16, at: String }`
    - `Slain { character: String, at: String }`
    - `Unknown { fingerprint: String, at: String }`
  - `pub fn parse_line(line: &str) -> RawEvent` — total function, never errors.
  - `pub fn fingerprint(message: &str) -> String` — digits → `#`, truncated to 48 chars (char-boundary safe).

- [ ] **Step 1: Crate scaffold + failing tests**

`crates/event_parser/Cargo.toml`:

```toml
[package]
name = "event_parser"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
```

Add `"crates/event_parser"` to workspace members.

`crates/event_parser/src/lib.rs` — types + stub + tests (stub returns `Unknown`):

```rust
//! Parses English Client.txt lines into typed events. Total: unknown
//! lines become fingerprinted Unknown events, never errors.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawEvent {
    AreaGenerated { area_id: String, area_level: u8, seed: u64, at: String },
    AreaEnteredName { display_name: String, at: String },
    LevelUp { character: String, class: String, level: u16, at: String },
    Slain { character: String, at: String },
    Unknown { fingerprint: String, at: String },
}

pub fn fingerprint(message: &str) -> String {
    let mapped: String = message
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect();
    mapped.chars().take(48).collect()
}

pub fn parse_line(line: &str) -> RawEvent {
    let at = line.get(..19).unwrap_or("").to_string();
    RawEvent::Unknown { fingerprint: fingerprint(line), at }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEN: &str = r#"2023/12/09 20:15:19 76512546 1186a0e0 [DEBUG Client 22384] Generating level 1 area "1_1_1" with seed 1780194933"#;
    const ENTER: &str = "2023/12/09 20:15:22 76515625 f22b6b26 [INFO Client 22384] : You have entered The Twilight Strand.";
    const LEVEL: &str = "2023/12/09 20:18:40 76713640 f22b6b26 [INFO Client 22384] : Wanderer (Ranger) is now level 2";
    const SLAIN: &str = "2023/12/09 20:20:01 76794001 f22b6b26 [INFO Client 22384] : Wanderer has been slain.";

    #[test]
    fn parses_area_generated() {
        assert_eq!(
            parse_line(GEN),
            RawEvent::AreaGenerated {
                area_id: "1_1_1".into(),
                area_level: 1,
                seed: 1780194933,
                at: "2023/12/09 20:15:19".into(),
            }
        );
    }

    #[test]
    fn parses_area_entered_name() {
        assert_eq!(
            parse_line(ENTER),
            RawEvent::AreaEnteredName {
                display_name: "The Twilight Strand".into(),
                at: "2023/12/09 20:15:22".into(),
            }
        );
    }

    #[test]
    fn parses_level_up_including_multiword_names() {
        assert_eq!(
            parse_line(LEVEL),
            RawEvent::LevelUp {
                character: "Wanderer".into(),
                class: "Ranger".into(),
                level: 2,
                at: "2023/12/09 20:18:40".into(),
            }
        );
    }

    #[test]
    fn parses_slain() {
        assert_eq!(
            parse_line(SLAIN),
            RawEvent::Slain {
                character: "Wanderer".into(),
                at: "2023/12/09 20:20:01".into(),
            }
        );
    }

    #[test]
    fn unknown_lines_are_fingerprinted_with_digits_masked() {
        let line = "2023/12/09 20:21:00 76800000 f22b6b26 [INFO Client 22384] : Trade accepted.";
        match parse_line(line) {
            RawEvent::Unknown { fingerprint: f, at } => {
                assert_eq!(at, "2023/12/09 20:21:00");
                assert!(f.contains("Trade accepted."));
                assert!(!f.chars().any(|c| c.is_ascii_digit()));
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn garbage_and_short_lines_never_panic() {
        for junk in ["", "x", ": ", "]] ", "no brackets here", "2023/12/09"] {
            let _ = parse_line(junk);
        }
    }

    #[test]
    fn area_names_with_apostrophes_survive() {
        let line = "2023/12/09 21:00:00 77000000 f22b6b26 [INFO Client 22384] : You have entered The Slaver's Pits.";
        assert_eq!(
            parse_line(line),
            RawEvent::AreaEnteredName {
                display_name: "The Slaver's Pits".into(),
                at: "2023/12/09 21:00:00".into(),
            }
        );
    }
}
```

Run: `cargo test -p event_parser` — parse tests FAIL (stub).

- [ ] **Step 2: Implement `parse_line`**

```rust
pub fn parse_line(line: &str) -> RawEvent {
    let at = line.get(..19).unwrap_or("").to_string();
    let unknown = |at: String| RawEvent::Unknown { fingerprint: fingerprint(line), at };

    let Some(bracket_end) = line.find("] ") else {
        return unknown(at);
    };
    let msg = &line[bracket_end + 2..];

    if let Some(rest) = msg.strip_prefix(": ") {
        if let Some(name) = rest
            .strip_prefix("You have entered ")
            .and_then(|s| s.strip_suffix('.'))
        {
            return RawEvent::AreaEnteredName { display_name: name.to_string(), at };
        }
        if let Some(idx) = rest.find(" is now level ") {
            let who = &rest[..idx];
            let level_str = &rest[idx + " is now level ".len()..];
            if let (Some(open), Some(close)) = (who.rfind(" ("), who.rfind(')')) {
                if close == who.len() - 1 && open + 2 < close {
                    if let Ok(level) = level_str.trim().parse::<u16>() {
                        return RawEvent::LevelUp {
                            character: who[..open].to_string(),
                            class: who[open + 2..close].to_string(),
                            level,
                            at,
                        };
                    }
                }
            }
        }
        if let Some(character) = rest.strip_suffix(" has been slain.") {
            return RawEvent::Slain { character: character.to_string(), at };
        }
        return unknown(at);
    }

    if let Some(rest) = msg.strip_prefix("Generating level ") {
        // rest: `1 area "1_1_1" with seed 1780194933`
        let parts: Option<RawEvent> = (|| {
            let (level_str, rest) = rest.split_once(" area \"")?;
            let (area_id, rest) = rest.split_once('"')?;
            let seed_str = rest.strip_prefix(" with seed ")?;
            Some(RawEvent::AreaGenerated {
                area_id: area_id.to_string(),
                area_level: level_str.trim().parse().ok()?,
                seed: seed_str.trim().parse().ok()?,
                at: at.clone(),
            })
        })();
        if let Some(ev) = parts {
            return ev;
        }
    }

    unknown(at)
}
```

Run: `cargo test -p event_parser` — all PASS. `cargo fmt --all` + clippy clean.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock crates/event_parser/
git commit -m "feat: event_parser crate for typed Client.txt events"
```

---

### Task 3: `session` crate — high-level session tracking

Pairing rule: a `Generating level N area "<id>"` line precedes its `You have entered <Name>.` line. The tracker holds the last `AreaGenerated`; when an `AreaEnteredName` arrives whose name matches that area's vendored display name, they pair into an authoritative `AreaEntered`. If no pending generated event matches (edge: log started mid-transition), fall back to resolving by display name: candidates from the area map with that exact name; pick the one whose act is closest to the current act (ties → lower act); if no candidate, emit nothing but record an `Unresolved` event. Seed tracking: entering an area whose seed differs from the last seen seed for that area id → `new_instance: true`.

**Files:**
- Create: `crates/session/Cargo.toml`, `crates/session/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `event_parser::RawEvent`, `content::game_data::{Area, AreaMap}`.
- Produces:
  - `session::SessionEvent` — `#[derive(Debug, Clone, PartialEq, Serialize)]`, variants:
    - `SessionStarted { at: String }` (emitted once, before the first area event)
    - `AreaEntered { area_id: String, display_name: String, act: u8, area_level: u8, is_town: bool, new_instance: bool, at: String }`
    - `LevelUp { character: String, class: String, level: u16, at: String }`
    - `Slain { character: String, at: String }`
    - `UnresolvedArea { display_name: String, at: String }`
  - `session::SessionTracker` — `pub fn new(areas: AreaMap) -> Self`, `pub fn on_raw(&mut self, event: &RawEvent) -> Vec<SessionEvent>`, `pub fn current_area_id(&self) -> Option<&str>`, `pub fn current_act(&self) -> Option<u8>`.

- [ ] **Step 1: Crate scaffold + failing tests**

`crates/session/Cargo.toml`:

```toml
[package]
name = "session"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
content = { path = "../content" }
event_parser = { path = "../event_parser" }
```

Add `"crates/session"` to workspace members.

`crates/session/src/lib.rs` — types + stub (`on_raw` returns empty) + tests:

```rust
//! Session tracking: pairs raw log events into authoritative area
//! transitions using the vendored area map.

use content::game_data::AreaMap;
use event_parser::RawEvent;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionStarted { at: String },
    AreaEntered {
        area_id: String,
        display_name: String,
        act: u8,
        area_level: u8,
        is_town: bool,
        new_instance: bool,
        at: String,
    },
    LevelUp { character: String, class: String, level: u16, at: String },
    Slain { character: String, at: String },
    UnresolvedArea { display_name: String, at: String },
}

struct PendingGenerated {
    area_id: String,
    area_level: u8,
    seed: u64,
}

pub struct SessionTracker {
    areas: AreaMap,
    started: bool,
    current_area_id: Option<String>,
    current_act: Option<u8>,
    pending: Option<PendingGenerated>,
    last_seed_by_area: std::collections::BTreeMap<String, u64>,
}

impl SessionTracker {
    pub fn new(areas: AreaMap) -> Self {
        Self {
            areas,
            started: false,
            current_area_id: None,
            current_act: None,
            pending: None,
            last_seed_by_area: std::collections::BTreeMap::new(),
        }
    }

    pub fn current_area_id(&self) -> Option<&str> {
        self.current_area_id.as_deref()
    }

    pub fn current_act(&self) -> Option<u8> {
        self.current_act
    }

    pub fn on_raw(&mut self, _event: &RawEvent) -> Vec<SessionEvent> {
        Vec::new() // stub
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::game_data::load_vendored;

    fn track(events: &[RawEvent]) -> (SessionTracker, Vec<SessionEvent>) {
        let (areas, _) = load_vendored().unwrap();
        let mut t = SessionTracker::new(areas);
        let mut out = Vec::new();
        for e in events {
            out.extend(t.on_raw(e));
        }
        (t, out)
    }

    fn gen(area_id: &str, level: u8, seed: u64) -> RawEvent {
        RawEvent::AreaGenerated {
            area_id: area_id.into(),
            area_level: level,
            seed,
            at: "t".into(),
        }
    }

    fn entered(name: &str) -> RawEvent {
        RawEvent::AreaEnteredName { display_name: name.into(), at: "t".into() }
    }

    #[test]
    fn pairs_generated_and_entered_into_area_entered() {
        let (t, out) = track(&[gen("1_1_1", 1, 42), entered("The Twilight Strand")]);
        assert_eq!(out[0], SessionEvent::SessionStarted { at: "t".into() });
        assert_eq!(
            out[1],
            SessionEvent::AreaEntered {
                area_id: "1_1_1".into(),
                display_name: "The Twilight Strand".into(),
                act: 1,
                area_level: 1,
                is_town: false,
                new_instance: true,
                at: "t".into(),
            }
        );
        assert_eq!(t.current_area_id(), Some("1_1_1"));
        assert_eq!(t.current_act(), Some(1));
    }

    #[test]
    fn town_flag_and_same_seed_revisit() {
        let (_, out) = track(&[
            gen("1_1_town", 1, 7),
            entered("Lioneye's Watch"),
            gen("1_1_2", 2, 500),
            entered("The Coast"),
            gen("1_1_town", 1, 7), // waypoint back, same instance
            entered("Lioneye's Watch"),
        ]);
        let entries: Vec<_> = out
            .iter()
            .filter_map(|e| match e {
                SessionEvent::AreaEntered { area_id, is_town, new_instance, .. } => {
                    Some((area_id.as_str(), *is_town, *new_instance))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            entries,
            vec![
                ("1_1_town", true, true),
                ("1_1_2", false, true),
                ("1_1_town", true, false),
            ]
        );
    }

    #[test]
    fn new_seed_means_new_instance() {
        let (_, out) = track(&[
            gen("1_1_2", 2, 500),
            entered("The Coast"),
            gen("1_1_2", 2, 501), // re-rolled instance
            entered("The Coast"),
        ]);
        let flags: Vec<bool> = out
            .iter()
            .filter_map(|e| match e {
                SessionEvent::AreaEntered { new_instance, .. } => Some(*new_instance),
                _ => None,
            })
            .collect();
        assert_eq!(flags, vec![true, true]);
    }

    #[test]
    fn name_only_fallback_prefers_current_act() {
        // "The Coast" exists in act 1 (1_1_2) and act 6 (2_6_2).
        // After entering an act 6 area, a name-only entry resolves to act 6.
        let (_, out) = track(&[
            gen("2_6_town", 40, 1),
            entered("Lioneye's Watch"),
            entered("The Coast"), // no Generating line captured
        ]);
        let last = out.last().unwrap();
        match last {
            SessionEvent::AreaEntered { area_id, act, .. } => {
                assert_eq!(area_id, "2_6_2");
                assert_eq!(*act, 6);
            }
            other => panic!("expected AreaEntered, got {other:?}"),
        }
    }

    #[test]
    fn unknown_display_name_yields_unresolved() {
        let (_, out) = track(&[entered("Some Future League Zone")]);
        assert!(matches!(
            out.last().unwrap(),
            SessionEvent::UnresolvedArea { .. }
        ));
    }

    #[test]
    fn levelup_and_slain_pass_through() {
        let (_, out) = track(&[
            RawEvent::LevelUp {
                character: "W".into(),
                class: "Ranger".into(),
                level: 5,
                at: "t".into(),
            },
            RawEvent::Slain { character: "W".into(), at: "t".into() },
        ]);
        assert!(matches!(out[1], SessionEvent::LevelUp { .. }));
        assert!(matches!(out[2], SessionEvent::Slain { .. }));
    }

    #[test]
    fn unknown_raw_events_produce_nothing() {
        let (_, out) = track(&[RawEvent::Unknown { fingerprint: "x".into(), at: "t".into() }]);
        // Only SessionStarted from first event
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], SessionEvent::SessionStarted { .. }));
    }
}
```

Note for the test `name_only_fallback_prefers_current_act`: verify the act-6 Coast id by checking vendored areas.json (`grep -B2 -A4 '"The Coast"' vendor/exile-leveling/data/areas.json`); if the actual act-6 id differs from `2_6_2`, use the real id in the test — the vendored data is the source of truth.

Run: `cargo test -p session` — expect FAIL (stub).

- [ ] **Step 2: Implement `on_raw`**

```rust
    pub fn on_raw(&mut self, event: &RawEvent) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        if !self.started {
            if let Some(at) = event_at(event) {
                self.started = true;
                out.push(SessionEvent::SessionStarted { at: at.to_string() });
            }
        }
        match event {
            RawEvent::AreaGenerated { area_id, area_level, seed, .. } => {
                self.pending = Some(PendingGenerated {
                    area_id: area_id.clone(),
                    area_level: *area_level,
                    seed: *seed,
                });
            }
            RawEvent::AreaEnteredName { display_name, at } => {
                out.extend(self.resolve_entry(display_name, at));
            }
            RawEvent::LevelUp { character, class, level, at } => {
                out.push(SessionEvent::LevelUp {
                    character: character.clone(),
                    class: class.clone(),
                    level: *level,
                    at: at.clone(),
                });
            }
            RawEvent::Slain { character, at } => {
                out.push(SessionEvent::Slain { character: character.clone(), at: at.clone() });
            }
            RawEvent::Unknown { .. } => {}
        }
        out
    }

    fn resolve_entry(&mut self, display_name: &str, at: &str) -> Vec<SessionEvent> {
        // Authoritative path: the pending Generating line names this area.
        let resolved: Option<(String, u8, u64)> = match self.pending.take() {
            Some(p) if self.areas.get(&p.area_id).is_some_and(|a| a.name == display_name) => {
                Some((p.area_id, p.area_level, p.seed))
            }
            other => {
                self.pending = other; // keep unrelated pending for later
                self.resolve_by_name(display_name)
            }
        };

        let Some((area_id, area_level, seed)) = resolved else {
            return vec![SessionEvent::UnresolvedArea {
                display_name: display_name.to_string(),
                at: at.to_string(),
            }];
        };

        let area = &self.areas[&area_id];
        let new_instance = self.last_seed_by_area.get(&area_id) != Some(&seed);
        self.last_seed_by_area.insert(area_id.clone(), seed);
        self.current_area_id = Some(area_id.clone());
        self.current_act = Some(area.act);

        vec![SessionEvent::AreaEntered {
            area_id,
            display_name: display_name.to_string(),
            act: area.act,
            area_level,
            is_town: area.is_town_area,
            new_instance,
            at: at.to_string(),
        }]
    }

    /// Fallback when no Generating line was captured: resolve by display
    /// name, preferring the candidate whose act is closest to the current
    /// act (ties break to the lower act). Seed 0 marks "unknown instance"
    /// (treated as new only if no prior seed is recorded).
    fn resolve_by_name(&self, display_name: &str) -> Option<(String, u8, u64)> {
        let current = self.current_act.unwrap_or(1) as i16;
        self.areas
            .values()
            .filter(|a| a.name == display_name)
            .min_by_key(|a| ((a.act as i16 - current).abs(), a.act))
            .map(|a| {
                let seed = self.last_seed_by_area.get(&a.id).copied().unwrap_or(0);
                (a.id.clone(), a.level.unwrap_or(0), seed)
            })
    }
```

Plus the helper at module level:

```rust
fn event_at(event: &RawEvent) -> Option<&str> {
    match event {
        RawEvent::AreaGenerated { at, .. }
        | RawEvent::AreaEnteredName { at, .. }
        | RawEvent::LevelUp { at, .. }
        | RawEvent::Slain { at, .. }
        | RawEvent::Unknown { at, .. } => Some(at),
    }
}
```

Note: `Area.level` is `Option<u8>` in `content::game_data` — if the field is not Option in the current code, match the real signature (read `crates/content/src/game_data.rs` first).

Run: `cargo test -p session` — all PASS. fmt + clippy clean.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock crates/session/
git commit -m "feat: session crate pairing log events into area transitions"
```

---

### Task 4: `replay` crate — harness, golden fixture, fake-play binary

**Files:**
- Create: `crates/replay/Cargo.toml`, `crates/replay/src/lib.rs`, `crates/replay/fixtures/act1-opening.log`, `crates/replay/tests/golden_act1.rs`, `crates/replay/src/bin/fake-play.rs`
- Modify: `Cargo.toml` (workspace members), `README.md`

**Interfaces:**
- Produces:
  - `replay::replay_lines<'a>(lines: impl IntoIterator<Item = &'a str>, areas: content::game_data::AreaMap) -> Vec<session::SessionEvent>`
  - `replay::replay_fixture(path: &std::path::Path) -> Result<Vec<session::SessionEvent>, ReplayError>` (loads vendored areas itself; `ReplayError` wraps io + `content::game_data::GameDataError`)
  - `replay::fixtures_dir() -> PathBuf`
  - Binary `fake-play`: `cargo run -p replay --bin fake-play -- <fixture> <target-file> [delay-ms]` — appends fixture lines to the target with the given delay (default 300 ms), printing progress; ends when the fixture is exhausted.

- [ ] **Step 1: Crate scaffold**

`crates/replay/Cargo.toml`:

```toml
[package]
name = "replay"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
content = { path = "../content" }
event_parser = { path = "../event_parser" }
session = { path = "../session" }
thiserror = "2"

[dev-dependencies]
input_log = { path = "../input_log" }
```

Add `"crates/replay"` to workspace members.

- [ ] **Step 2: Write the golden fixture**

`crates/replay/fixtures/act1-opening.log` — a realistic act-1 opening (line shapes per Global Constraints; PIDs/counters arbitrary):

```
2026/07/21 19:00:01 1000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_1" with seed 101
2026/07/21 19:00:03 3000 f22b6b26 [INFO Client 900] : You have entered The Twilight Strand.
2026/07/21 19:02:10 130000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 2
2026/07/21 19:03:20 200000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_town" with seed 555
2026/07/21 19:03:22 202000 f22b6b26 [INFO Client 900] : You have entered Lioneye's Watch.
2026/07/21 19:04:00 240000 1186a0e0 [DEBUG Client 900] Generating level 2 area "1_1_2" with seed 8801
2026/07/21 19:04:02 242000 f22b6b26 [INFO Client 900] : You have entered The Coast.
2026/07/21 19:05:00 300000 f22b6b26 [INFO Client 900] : Wanderer (Ranger) is now level 3
2026/07/21 19:06:00 360000 1186a0e0 [DEBUG Client 900] Generating level 1 area "1_1_town" with seed 555
2026/07/21 19:06:02 362000 f22b6b26 [INFO Client 900] : You have entered Lioneye's Watch.
2026/07/21 19:07:00 420000 1186a0e0 [DEBUG Client 900] Generating level 2 area "1_1_2" with seed 8801
2026/07/21 19:07:02 422000 f22b6b26 [INFO Client 900] : You have entered The Coast.
2026/07/21 19:08:30 510000 f22b6b26 [INFO Client 900] : AFK mode is now ON. Autoreply "afk"
2026/07/21 19:09:00 540000 1186a0e0 [DEBUG Client 900] Generating level 3 area "1_1_3" with seed 9911
2026/07/21 19:09:02 542000 f22b6b26 [INFO Client 900] : You have entered The Mud Flats.
2026/07/21 19:10:00 600000 f22b6b26 [INFO Client 900] : Wanderer has been slain.
2026/07/21 19:10:20 620000 1186a0e0 [DEBUG Client 900] Generating level 3 area "1_1_3" with seed 9911
2026/07/21 19:10:22 622000 f22b6b26 [INFO Client 900] : You have entered The Mud Flats.
```

- [ ] **Step 3: Failing golden test**

`crates/replay/tests/golden_act1.rs`:

```rust
use replay::{fixtures_dir, replay_fixture};
use session::SessionEvent;

#[test]
fn act1_opening_golden_sequence() {
    let events = replay_fixture(&fixtures_dir().join("act1-opening.log")).unwrap();

    let compact: Vec<String> = events
        .iter()
        .map(|e| match e {
            SessionEvent::SessionStarted { .. } => "start".to_string(),
            SessionEvent::AreaEntered { area_id, is_town, new_instance, .. } => format!(
                "enter:{area_id}:{}:{}",
                if *is_town { "town" } else { "field" },
                if *new_instance { "new" } else { "revisit" }
            ),
            SessionEvent::LevelUp { level, .. } => format!("level:{level}"),
            SessionEvent::Slain { .. } => "slain".to_string(),
            SessionEvent::UnresolvedArea { display_name, .. } => {
                format!("unresolved:{display_name}")
            }
        })
        .collect();

    assert_eq!(
        compact,
        vec![
            "start",
            "enter:1_1_1:field:new",
            "level:2",
            "enter:1_1_town:town:new",
            "enter:1_1_2:field:new",
            "level:3",
            "enter:1_1_town:town:revisit",
            "enter:1_1_2:field:revisit",
            "enter:1_1_3:field:new",
            "slain",
            "enter:1_1_3:field:revisit",
        ]
    );
}
```

Run: `cargo test -p replay` — FAIL (lib missing).

- [ ] **Step 4: Implement the lib**

`crates/replay/src/lib.rs`:

```rust
//! Deterministic replay of Client.txt fixtures through the full
//! parse -> session pipeline. Used by tests and the fake-play binary.

use std::path::{Path, PathBuf};

use content::game_data::{load_vendored, AreaMap, GameDataError};
use session::{SessionEvent, SessionTracker};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("failed to read fixture: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to load game data: {0}")]
    GameData(#[from] GameDataError),
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn replay_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    areas: AreaMap,
) -> Vec<SessionEvent> {
    let mut tracker = SessionTracker::new(areas);
    let mut out = Vec::new();
    for line in lines {
        let raw = event_parser::parse_line(line);
        out.extend(tracker.on_raw(&raw));
    }
    out
}

pub fn replay_fixture(path: &Path) -> Result<Vec<SessionEvent>, ReplayError> {
    let (areas, _) = load_vendored()?;
    let text = std::fs::read_to_string(path)?;
    Ok(replay_lines(text.lines(), areas))
}
```

Run: `cargo test -p replay` — golden test PASS. If the sequence mismatches, debug via the session/parser semantics — do not edit the expected sequence to match broken output; the expected sequence above is derived directly from the fixture and the Task 3 pairing rules.

- [ ] **Step 5: fake-play binary**

`crates/replay/src/bin/fake-play.rs`:

```rust
//! Simulates live play by appending fixture lines to a target file.
//! Usage: fake-play <fixture> <target-file> [delay-ms]

use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (fixture, target) = match (args.get(1), args.get(2)) {
        (Some(f), Some(t)) => (f.clone(), t.clone()),
        _ => {
            eprintln!("usage: fake-play <fixture> <target-file> [delay-ms]");
            std::process::exit(2);
        }
    };
    let delay_ms: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(300);

    let text = std::fs::read_to_string(&fixture)?;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)?;

    let total = text.lines().count();
    for (i, line) in text.lines().enumerate() {
        writeln!(out, "{line}")?;
        out.flush()?;
        println!("[{}/{total}] {line}", i + 1);
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
    println!("done: {total} lines appended to {target}");
    Ok(())
}
```

Manual check: `cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 10` prints 18 progress lines; run twice, file grows. Remove `/tmp/fake-client.txt` after.

- [ ] **Step 6: End-to-end integration test (tailer → parser → session)**

`crates/replay/tests/end_to_end_tail.rs`:

```rust
use std::io::Write;
use std::time::Duration;

use content::game_data::load_vendored;
use input_log::FilePoller;
use replay::{fixtures_dir, replay_fixture};
use session::SessionTracker;

#[test]
fn tailing_a_growing_file_matches_direct_replay() {
    let fixture = fixtures_dir().join("act1-opening.log");
    let expected = replay_fixture(&fixture).unwrap();

    let path = std::env::temp_dir().join(format!("poe-copilot-e2e-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, "").unwrap();

    let (areas, _) = load_vendored().unwrap();
    let mut tracker = SessionTracker::new(areas);
    let mut poller = FilePoller::new(path.clone(), false).unwrap();
    let mut got = Vec::new();

    let text = std::fs::read_to_string(&fixture).unwrap();
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    for line in text.lines() {
        // Write each line in two chunks to exercise partial-line handling.
        let mid = line.len() / 2;
        write!(f, "{}", &line[..mid]).unwrap();
        f.flush().unwrap();
        for l in poller.poll().unwrap() {
            got.extend(tracker.on_raw(&event_parser::parse_line(&l)));
        }
        writeln!(f, "{}", &line[mid..]).unwrap();
        f.flush().unwrap();
        for l in poller.poll().unwrap() {
            got.extend(tracker.on_raw(&event_parser::parse_line(&l)));
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(got, expected);
    let _ = std::fs::remove_file(&path);
}
```

Add `event_parser` to `[dev-dependencies]` too (it is already a regular dependency — regular is fine, no change needed if so).

Run: `cargo test -p replay` — both tests PASS. Full gate: `cargo test --workspace`, fmt, clippy — clean.

- [ ] **Step 7: README note + commit**

Append to the README "Development" section:

```markdown
Simulate a play session on any OS (no game needed):

    cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 300

Point the app's log tailer at `/tmp/fake-client.txt` to watch events flow.
```

```bash
git add Cargo.toml Cargo.lock crates/replay/ README.md
git commit -m "feat: replay harness, golden act-1 fixture, and fake-play simulator"
```

---

## Verification (end of plan)

- [ ] `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — green (now including input_log, event_parser, session, replay).
- [ ] `cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 10` — works; clean up the temp file.
- [ ] CI green on both OSes after push (Windows path/temp-file behavior is exercised by the poller tests on the Windows runner).

## Self-Review Notes

- Spec coverage: FR-01 partial (path selection UI comes with the app plan; the poller takes an explicit path), FR-02 ✅ (partial lines, truncation, replacement, restart via offset reset; duplicate-notification immunity is inherent to offset-based polling), FR-03 ✅ (area IDs from Generating lines; name fallback), FR-13 ✅ (deterministic replay + fixtures), fake-play ✅ (spec §7's simulator).
- Type consistency: `RawEvent` (Task 2) consumed by Tasks 3–4 by name; `SessionEvent`/`SessionTracker` (Task 3) consumed by Task 4; `FilePoller` (Task 1) consumed by Task 4's e2e test.
- Known open risk: log-line shapes are from community documentation, not a captured log from this machine. The Windows validation session will replay a REAL Client.txt through this pipeline; fixtures get corrected to match reality if needed (never the reverse).
