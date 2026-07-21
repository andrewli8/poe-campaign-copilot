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
        Ok(Self {
            path,
            offset,
            assembler: LineAssembler::new(),
        })
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
        let mut buf = Vec::with_capacity((len - self.offset) as usize);
        file.take(len - self.offset).read_to_end(&mut buf)?;
        self.offset = len;
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
}
