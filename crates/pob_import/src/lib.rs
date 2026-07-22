//! Decodes Path of Building share codes/XML into structured leveling build
//! plans, enriched against vendored gem data. Total: unparseable or
//! unenriched builds degrade to `Reliability::Unsupported` rather than
//! erroring — only a genuinely malformed share code or XML document is an
//! error.

mod decode;
mod parse;

pub use decode::decode_share_code;
pub use parse::parse_build;

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reliability {
    Explicit,
    Structured,
    Inferred,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GemPlan {
    pub name: String,
    pub required_level: Option<u8>,
    pub is_support: Option<bool>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkillSetPlan {
    pub title: String,
    pub level_range: Option<(u16, u16)>,
    pub gems: Vec<GemPlan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Milestone {
    pub level: u16,
    pub label: String,
    pub reliability: Reliability,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LevelingBuildPlan {
    pub class_name: String,
    pub ascend_name: Option<String>,
    pub skill_sets: Vec<SkillSetPlan>,
    pub passive_spec_titles: Vec<String>,
    pub notes: Option<String>,
    pub milestones: Vec<Milestone>,
    pub reliability: Reliability,
}

#[derive(Debug, Error)]
pub enum PobError {
    #[error("input is not a Path of Building share code")]
    NotACode,
    #[error("URLs are not supported; paste the raw PoB share code or XML instead")]
    UrlNotSupported,
    #[error("failed to decode share code: {0}")]
    Decode(String),
    #[error("failed to parse build XML: {0}")]
    Xml(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::game_data::load_vendored_gems;

    fn fixture_xml() -> String {
        std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/leveling-build.xml"),
        )
        .unwrap()
    }

    fn encode(xml: &str) -> String {
        use base64::Engine as _;
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write as _;
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(xml.as_bytes()).unwrap();
        base64::engine::general_purpose::URL_SAFE.encode(e.finish().unwrap())
    }

    #[test]
    fn share_code_roundtrip_and_xml_passthrough() {
        let xml = fixture_xml();
        assert_eq!(decode_share_code(&encode(&xml)).unwrap(), xml);
        assert_eq!(decode_share_code(&xml).unwrap(), xml);
        assert!(matches!(
            decode_share_code("https://pobb.in/abc"),
            Err(PobError::UrlNotSupported)
        ));
        assert!(decode_share_code("!!!not-base64!!!").is_err());
    }

    #[test]
    fn parses_fixture_into_plan() {
        let gems = load_vendored_gems().unwrap();
        let plan = parse_build(&fixture_xml(), &gems).unwrap();
        assert_eq!(plan.class_name, "Ranger");
        assert_eq!(plan.ascend_name.as_deref(), Some("Deadeye"));
        assert_eq!(plan.skill_sets.len(), 2);
        assert_eq!(plan.skill_sets[0].level_range, Some((1, 12)));
        assert_eq!(plan.skill_sets[1].level_range, Some((13, 32)));
        assert_eq!(plan.passive_spec_titles, vec!["Level 20", "Level 45"]);
        assert!(plan.notes.as_deref().unwrap().contains("Toxic Rain"));
        assert_eq!(plan.reliability, Reliability::Structured);

        // Milestones: set switch at 13, plus gem availabilities (e.g.
        // Frostblink at its vendored required_level 4).
        assert!(
            plan.milestones
                .iter()
                .any(|m| m.level == 13 && m.label.contains("13-32"))
        );
        assert!(
            plan.milestones
                .iter()
                .any(|m| m.label == "Gem available: Frostblink" && m.level == 4)
        );
        // Support gems excluded from gem milestones.
        assert!(
            !plan
                .milestones
                .iter()
                .any(|m| m.label.contains("Mirage Archer"))
        );
        // Sorted by level.
        let levels: Vec<u16> = plan.milestones.iter().map(|m| m.level).collect();
        let mut sorted = levels.clone();
        sorted.sort();
        assert_eq!(levels, sorted);
    }

    #[test]
    fn support_gem_without_suffix_resolves_via_fallback() {
        let gems = load_vendored_gems().unwrap();
        let plan = parse_build(&fixture_xml(), &gems).unwrap();

        // Real-export shape: nameSpec="Pierce" (no " Support" suffix) still
        // enriches via the "{name} Support" fallback.
        let pierce = plan
            .skill_sets
            .iter()
            .flat_map(|s| &s.gems)
            .find(|g| g.name == "Pierce")
            .expect("fixture has a Pierce gem in the real-export shape");
        assert_eq!(pierce.is_support, Some(true));
        assert_eq!(pierce.required_level, Some(4));

        // Support gems (whether looked up via exact match or the fallback)
        // never produce a milestone.
        assert!(!plan.milestones.iter().any(|m| m.label.contains("Pierce")));
    }

    #[test]
    fn unparseable_build_is_unsupported_not_error() {
        let gems = load_vendored_gems().unwrap();
        let plan = parse_build(
            "<PathOfBuilding><Build className=\"Witch\"/></PathOfBuilding>",
            &gems,
        )
        .unwrap();
        assert_eq!(plan.reliability, Reliability::Unsupported);
        assert!(plan.milestones.is_empty());
    }
}
