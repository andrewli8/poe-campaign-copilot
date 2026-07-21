use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::assembler::LineAssembler;

/// Maximum number of bytes read from the file in a single `poll()` call.
/// Real Client.txt files can reach hundreds of MB; bounding the per-poll
/// read keeps memory use predictable. Callers polling in a loop drain
/// large backlogs incrementally across multiple calls.
pub const MAX_POLL_BYTES: u64 = 1_048_576; // 1 MiB

pub struct FilePoller {
    path: PathBuf,
    offset: u64,
    assembler: LineAssembler,
    max_poll_bytes: u64,
}

impl FilePoller {
    pub fn new(path: PathBuf, start_at_end: bool) -> std::io::Result<Self> {
        let offset = if start_at_end {
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        Ok(Self {
            path,
            offset,
            assembler: LineAssembler::new(),
            max_poll_bytes: MAX_POLL_BYTES,
        })
    }

    /// Overrides the per-poll read cap (default [`MAX_POLL_BYTES`]).
    /// Primarily useful for tests that want to exercise incremental
    /// draining without writing huge fixture files.
    pub fn with_max_poll_bytes(mut self, cap: u64) -> Self {
        self.max_poll_bytes = cap;
        self
    }

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
        let to_read = (len - self.offset).min(self.max_poll_bytes);
        let mut buf = Vec::with_capacity(to_read as usize);
        file.take(to_read).read_to_end(&mut buf)?;
        self.offset += buf.len() as u64;
        Ok(self.assembler.feed(&buf))
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
        std::env::temp_dir().join(format!("poe-copilot-test-{}-{n}.log", std::process::id()))
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

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
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
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
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

    #[test]
    fn bounded_poll_drains_backlog_incrementally() {
        let path = temp_path();
        let _c = Cleanup(path.clone());
        let lines = vec![
            "aaaaaaaa".to_string(),
            "bbbbbbbb".to_string(),
            "cccccccc".to_string(),
        ];
        let content = lines.iter().map(|l| format!("{l}\n")).collect::<String>();
        assert!(content.len() as u64 > 8, "test content must exceed cap");
        std::fs::write(&path, &content).unwrap();

        let file_len = content.len() as u64;
        let mut p = FilePoller::new(path.clone(), false)
            .unwrap()
            .with_max_poll_bytes(8);

        let mut got = Vec::new();
        let mut polls = 0;
        // Poll until the reader has caught up to the end of the file. Each
        // poll advances offset by at most the (small) cap, so this proves
        // the backlog is drained incrementally rather than in one read.
        while p.offset < file_len {
            got.extend(p.poll().unwrap());
            polls += 1;
            if polls > 100 {
                panic!("poll() did not converge; possible infinite loop");
            }
        }
        assert!(
            polls > 1,
            "expected multiple polls to drain backlog with a small cap, got {polls}"
        );
        assert_eq!(got, lines);
    }

    #[test]
    fn truncation_discards_stale_partial_line() {
        let path = temp_path();
        let _c = Cleanup(path.clone());
        std::fs::write(&path, "aaaa\nbb").unwrap(); // "bb" has no trailing newline
        let mut p = FilePoller::new(path.clone(), false).unwrap();
        assert_eq!(p.poll().unwrap(), vec!["aaaa".to_string()]); // "bb" held as partial

        std::fs::write(&path, "cc\n").unwrap(); // shorter: truncation/replacement
        assert_eq!(p.poll().unwrap(), vec!["cc".to_string()]);
    }
}
