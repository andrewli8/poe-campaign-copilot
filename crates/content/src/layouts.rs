//! Zone layout content (notes + diagram images) extracted from the
//! community poelayouts compilation. See tools/extract_poelayouts.py.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Unaudited,
    Verified,
    Outdated,
    Corrected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audit {
    pub status: AuditStatus,
    pub verified_patch: Option<String>,
    pub correction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSource {
    pub document: String,
    pub images_author: String,
    pub notes_author: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEntry {
    pub area_id: String,
    pub act: u8,
    pub display_name: String,
    pub docx_headings: Vec<String>,
    pub descriptions: Vec<String>,
    pub notes: Vec<String>,
    pub images: Vec<String>,
    pub source: LayoutSource,
    pub audit: Audit,
}

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("failed to read layout content: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse layout entry {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}

pub fn layouts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content")
        .join("layouts")
}

pub fn load_all_layouts() -> Result<Vec<LayoutEntry>, LayoutError> {
    let mut paths = Vec::new();
    for act_dir in std::fs::read_dir(layouts_dir())? {
        let act_dir = act_dir?.path();
        let is_act_dir = act_dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("act-"));
        if !is_act_dir || !act_dir.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&act_dir)? {
            let p = f?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                paths.push(p);
            }
        }
    }
    paths.sort();

    let mut entries = Vec::with_capacity(paths.len());
    for p in paths {
        let text = std::fs::read_to_string(&p)?;
        let entry = serde_json::from_str(&text).map_err(|source| LayoutError::Json {
            path: p.display().to_string(),
            source,
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_status_roundtrips_lowercase() {
        let s: AuditStatus = serde_json::from_str("\"unaudited\"").unwrap();
        assert_eq!(s, AuditStatus::Unaudited);
        assert_eq!(
            serde_json::to_string(&AuditStatus::Verified).unwrap(),
            "\"verified\""
        );
    }
}
