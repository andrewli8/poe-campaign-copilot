//! Compiles vendored route data into versioned content-pack JSON.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game_data::{GameDataError, load_vendored};
use crate::layouts::{LayoutEntry, LayoutError, load_all_layouts, load_extraction_meta};
use crate::route_dsl::{ParseError, parse_route_file};
use crate::vendor::read_act_route;
use crate::walk::{CompiledStep, WalkError, WalkState, walk_act};

pub const EXILE_LEVELING_REF: &str = "e9a248c4a452f58da6e0f30751b20072ad3276cd";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Variant {
    LeagueStart,
    Standard,
}

impl Variant {
    pub fn defines(&self) -> BTreeSet<String> {
        match self {
            Variant::LeagueStart => ["LEAGUE_START".to_string()].into_iter().collect(),
            Variant::Standard => BTreeSet::new(),
        }
    }

    pub fn file_stem(&self) -> &'static str {
        match self {
            Variant::LeagueStart => "standard-fast.league-start",
            Variant::Standard => "standard-fast.standard",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Variant::LeagueStart => "league_start",
            Variant::Standard => "standard",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub project: String,
    pub reference: String,
    pub license: String,
}

#[derive(Debug, Serialize)]
pub struct ActRoute {
    pub act: u8,
    pub steps: Vec<CompiledStep>,
}

#[derive(Debug, Serialize)]
pub struct ContentPack {
    pub format_version: u32,
    pub source: SourceInfo,
    pub variant: String,
    pub acts: Vec<ActRoute>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutPack {
    pub format_version: u32,
    pub source: SourceInfo,
    pub entries: Vec<LayoutEntry>,
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("route parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("route walk error: {0}")]
    Walk(#[from] WalkError),
    #[error("game data error: {0}")]
    GameData(#[from] GameDataError),
    #[error("layout content error: {0}")]
    Layout(#[from] LayoutError),
}

pub fn compile_route_pack(variant: Variant) -> Result<ContentPack, CompileError> {
    let (areas, quests) = load_vendored()?;
    let defines = variant.defines();
    let mut state = WalkState::campaign_start();
    let mut acts = Vec::with_capacity(10);

    for act in 1..=10u8 {
        let sections = parse_route_file(&read_act_route(act)?, &defines)?;
        let steps = walk_act(act, &sections, &areas, &quests, &mut state)?;
        acts.push(ActRoute { act, steps });
    }

    Ok(ContentPack {
        format_version: 1,
        source: SourceInfo {
            project: "HeartofPhos/exile-leveling".to_string(),
            reference: EXILE_LEVELING_REF.to_string(),
            license: "MIT".to_string(),
        },
        variant: variant.label().to_string(),
        acts,
    })
}

pub fn compile_layout_pack() -> Result<LayoutPack, CompileError> {
    let entries = load_all_layouts()?;
    let meta = load_extraction_meta()?;
    let sha_prefix: String = meta.docx_sha256.chars().take(12).collect();
    Ok(LayoutPack {
        format_version: 1,
        source: SourceInfo {
            project: "poelayouts community compilation".to_string(),
            reference: format!("poelayouts.docx sha256:{sha_prefix}"),
            license: "used with credit — see CREDITS.md".to_string(),
        },
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_both_variants_across_all_acts() {
        for variant in [Variant::LeagueStart, Variant::Standard] {
            let pack = compile_route_pack(variant).expect("pack compiles");
            assert_eq!(pack.format_version, 1);
            assert_eq!(pack.acts.len(), 10);
            for act_route in &pack.acts {
                assert!(
                    !act_route.steps.is_empty(),
                    "act {} has no steps",
                    act_route.act
                );
            }
            // serializes without error and includes tagged fragments
            let json = serde_json::to_string(&pack).unwrap();
            assert!(json.contains("\"type\":\"enter\""));
            assert!(json.contains("\"area_context\""));
        }
    }

    /// `compile_route_pack` must thread a single `WalkState` continuously
    /// across acts 1-10. A regression that resets state per act would still
    /// compile without error (every act still walks fine in isolation), but
    /// act 2+ would silently get wrong area contexts. This test
    /// independently reproduces the walk with its own single `WalkState`
    /// (the same code path the compiler claims to use) and asserts the
    /// compiled pack matches it, plus a belt-and-braces check that act 2+
    /// doesn't start from the act-1 starting area.
    #[test]
    fn cross_act_state_continuity() {
        let variant = Variant::LeagueStart;
        let pack = compile_route_pack(variant).expect("pack compiles");

        let (areas, quests) = load_vendored().expect("vendored data loads");
        let defines = variant.defines();
        let mut state = WalkState::campaign_start();

        for act in 1..=10u8 {
            let sections = parse_route_file(&read_act_route(act).unwrap(), &defines).unwrap();
            let steps = walk_act(act, &sections, &areas, &quests, &mut state)
                .unwrap_or_else(|e| panic!("act {act} walk failed: {e}"));

            let expected_first_context = steps[0].area_context.clone();
            let actual_first_context = &pack.acts[(act - 1) as usize].steps[0].area_context;

            assert_eq!(
                actual_first_context, &expected_first_context,
                "act {act} first step area_context diverged from independently \
                 walked state; compiler is not threading WalkState continuously \
                 across acts"
            );

            if act >= 2 {
                assert_ne!(
                    actual_first_context, "1_1_1",
                    "act {act} first step area_context is the act-1 starting area \
                     ('1_1_1'), indicating WalkState was reset per act"
                );
            }
        }
    }

    #[test]
    fn compiles_layout_pack() {
        let pack = compile_layout_pack().expect("layout pack compiles");
        assert_eq!(pack.format_version, 1);
        assert!(crate::layouts::EXPECTED_ENTRY_COUNT_RANGE.contains(&pack.entries.len()));
        assert!(
            pack.source.reference.starts_with("poelayouts.docx sha256:"),
            "unexpected reference: {}",
            pack.source.reference
        );
        let json = serde_json::to_string(&pack).unwrap();
        assert!(json.contains("\"first_verified_patch\""));
        assert!(json.contains("\"unaudited\""));
    }
}
