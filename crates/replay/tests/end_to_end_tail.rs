use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use content::game_data::load_vendored;
use input_log::FilePoller;
use replay::{fixtures_dir, replay_fixture};
use session::SessionTracker;

/// RAII guard that removes the temp file even if the test panics.
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn tailing_a_growing_file_matches_direct_replay() {
    let fixture = fixtures_dir().join("act1-opening.log");
    let expected = replay_fixture(&fixture).unwrap();

    let path = std::env::temp_dir().join(format!("poe-copilot-e2e-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let _c = Cleanup(path.clone());
    std::fs::write(&path, "").unwrap();

    let (areas, _) = load_vendored().unwrap();
    let mut tracker = SessionTracker::new(areas);
    let mut poller = FilePoller::new(path.clone(), false).unwrap();
    let mut got = Vec::new();

    let text = std::fs::read_to_string(&fixture).unwrap();
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
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
}
