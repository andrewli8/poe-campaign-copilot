//! Deterministic replay of Client.txt fixtures through the full
//! parse -> session pipeline. Used by tests and the fake-play binary.

use std::path::{Path, PathBuf};

use content::game_data::{AreaMap, GameDataError, load_vendored};
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
