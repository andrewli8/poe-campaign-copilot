//! Zone layout content (notes + diagram images) extracted from the
//! community poelayouts compilation. See tools/extract_poelayouts.py.
//!
//! Schema v2: every note, description, and image carries its own audit
//! record (Plan 2 spec §4) rather than one audit record per entry — a zone
//! can have some notes verified and others not.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Sanity bounds on the number of extracted layout entries. Both the
/// extractor's output and the compiled pack are expected to fall in this
/// range; a count outside it likely indicates a parsing regression.
pub const EXPECTED_ENTRY_COUNT_RANGE: std::ops::RangeInclusive<usize> = 120..=132;

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
    pub first_verified_patch: Option<String>,
    pub last_verified_patch: Option<String>,
    pub correction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditedText {
    pub text: String,
    pub audit: Audit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditedImage {
    pub file: String,
    pub audit: Audit,
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
    pub descriptions: Vec<AuditedText>,
    pub notes: Vec<AuditedText>,
    pub images: Vec<AuditedImage>,
    pub source: LayoutSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionMeta {
    pub docx_sha256: String,
    pub tool: String,
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
    #[error("failed to parse extraction metadata {path}: {source}")]
    MetaJson {
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

pub fn load_extraction_meta() -> Result<ExtractionMeta, LayoutError> {
    let path = layouts_dir().join("extraction-meta.json");
    let text = std::fs::read_to_string(&path)?;
    serde_json::from_str(&text).map_err(|source| LayoutError::MetaJson {
        path: path.display().to_string(),
        source,
    })
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
