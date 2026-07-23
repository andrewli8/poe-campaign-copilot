//! Stateful walk over parsed route sections: resolves the player's area
//! context for every step and validates area/quest references.

use serde::Serialize;
use thiserror::Error;

use crate::game_data::{AreaMap, QuestMap};
use crate::route_dsl::{Fragment, Section};

#[derive(Debug, Clone)]
pub struct WalkState {
    pub current_area_id: String,
    pub last_town_area_id: String,
    pub portal_area_id: Option<String>,
}

impl WalkState {
    pub fn campaign_start() -> Self {
        Self {
            current_area_id: "1_1_1".to_string(),
            last_town_area_id: "1_1_town".to_string(),
            portal_area_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompiledStep {
    pub id: String,
    pub act: u8,
    pub section: String,
    pub area_context: String,
    pub fragments: Vec<Fragment>,
    pub sub_steps: Vec<Vec<Fragment>>,
}

#[derive(Debug, Error)]
pub enum WalkError {
    #[error("step {step_id}: unknown area id '{area_id}'")]
    UnknownArea { step_id: String, area_id: String },
    #[error("step {step_id}: unknown quest id '{quest_id}'")]
    UnknownQuest { step_id: String, quest_id: String },
    #[error("step {step_id}: portal misuse: {detail}")]
    PortalMisuse { step_id: String, detail: String },
}

pub fn walk_act(
    act: u8,
    sections: &[Section],
    areas: &AreaMap,
    quests: &QuestMap,
    state: &mut WalkState,
) -> Result<Vec<CompiledStep>, WalkError> {
    let mut compiled = Vec::new();
    let mut index = 0usize;

    for section in sections {
        for step in &section.steps {
            index += 1;
            let step_id = format!("a{act}-s{index:03}");
            let area_context = state.current_area_id.clone();

            for fragment in &step.fragments {
                apply_fragment(fragment, areas, quests, state, &step_id)?;
            }
            for sub in &step.sub_steps {
                for fragment in sub {
                    apply_fragment(fragment, areas, quests, state, &step_id)?;
                }
            }

            let fragments = step
                .fragments
                .iter()
                .map(|f| expand_quest_offers(f, quests))
                .collect();
            let sub_steps = step
                .sub_steps
                .iter()
                .map(|sub| sub.iter().map(|f| expand_quest_offers(f, quests)).collect())
                .collect();

            compiled.push(CompiledStep {
                id: step_id,
                act,
                section: section.name.clone(),
                area_context,
                fragments,
                sub_steps,
            });
        }
    }

    Ok(compiled)
}

/// Mirrors exile-leveling's EvaluateQuest (fragment/index.ts): a quest
/// hand-in written without explicit reward offer ids means "collect every
/// reward offer of this quest", so the compiled fragment carries all offer
/// ids from quests.json. Explicit ids are preserved verbatim. Returns a new
/// fragment; never mutates the parsed source.
fn expand_quest_offers(fragment: &Fragment, quests: &QuestMap) -> Fragment {
    match fragment {
        Fragment::Quest {
            quest_id,
            reward_offer_ids,
        } if reward_offer_ids.is_empty() => {
            let expanded = quests
                .get(quest_id)
                .map(|q| q.reward_offers.keys().cloned().collect())
                .unwrap_or_default();
            Fragment::Quest {
                quest_id: quest_id.clone(),
                reward_offer_ids: expanded,
            }
        }
        other => other.clone(),
    }
}

fn transition(state: &mut WalkState, areas: &AreaMap, area_id: &str) {
    if let Some(area) = areas.get(area_id)
        && area.is_town_area
    {
        state.last_town_area_id = area.id.clone();
    }
    state.current_area_id = area_id.to_string();
}

fn require_area<'a>(
    areas: &'a AreaMap,
    area_id: &str,
    step_id: &str,
) -> Result<&'a crate::game_data::Area, WalkError> {
    areas.get(area_id).ok_or_else(|| WalkError::UnknownArea {
        step_id: step_id.to_string(),
        area_id: area_id.to_string(),
    })
}

fn apply_fragment(
    fragment: &Fragment,
    areas: &AreaMap,
    quests: &QuestMap,
    state: &mut WalkState,
    step_id: &str,
) -> Result<(), WalkError> {
    match fragment {
        Fragment::Enter { area_id } | Fragment::WaypointUse { area_id } => {
            require_area(areas, area_id, step_id)?;
            transition(state, areas, area_id);
        }
        Fragment::Area { area_id }
        | Fragment::Crafting {
            area_id: Some(area_id),
        } => {
            require_area(areas, area_id, step_id)?;
        }
        Fragment::Logout | Fragment::Ascend { .. } => {
            let town = state.last_town_area_id.clone();
            transition(state, areas, &town);
            if matches!(fragment, Fragment::Logout) {
                state.portal_area_id = None;
            }
        }
        Fragment::PortalSet => {
            let current = require_area(areas, &state.current_area_id.clone(), step_id)?;
            if current.is_town_area {
                return Err(WalkError::PortalMisuse {
                    step_id: step_id.to_string(),
                    detail: "portal cannot be set in town".to_string(),
                });
            }
            state.portal_area_id = Some(state.current_area_id.clone());
        }
        Fragment::PortalUse => {
            let current_id = state.current_area_id.clone();
            let current = require_area(areas, &current_id, step_id)?;
            // Mirrors exile-leveling's evaluator (fragment/index.ts, EvaluatePortal):
            // using a portal from a non-town area re-anchors the portal to the current
            // area even if one was already set elsewhere. Intentionally broader than
            // "set only when unset" — do not "fix" without checking upstream.
            if state.portal_area_id.as_deref() != Some(current_id.as_str()) && !current.is_town_area
            {
                state.portal_area_id = Some(current_id.clone());
            }
            let portal_id =
                state
                    .portal_area_id
                    .clone()
                    .ok_or_else(|| WalkError::PortalMisuse {
                        step_id: step_id.to_string(),
                        detail: "portal not set".to_string(),
                    })?;
            let portal_area = require_area(areas, &portal_id, step_id)?;

            if current_id == portal_id {
                let town = portal_area.parent_town_area_id.clone().ok_or_else(|| {
                    WalkError::PortalMisuse {
                        step_id: step_id.to_string(),
                        detail: format!("area {portal_id} has no parent town"),
                    }
                })?;
                transition(state, areas, &town);
            } else if portal_area.parent_town_area_id.as_deref() == Some(current_id.as_str()) {
                transition(state, areas, &portal_id);
                state.portal_area_id = None;
            } else {
                return Err(WalkError::PortalMisuse {
                    step_id: step_id.to_string(),
                    detail: "portal used from unrelated area".to_string(),
                });
            }
        }
        Fragment::Quest { quest_id, .. } if !quests.contains_key(quest_id) => {
            return Err(WalkError::UnknownQuest {
                step_id: step_id.to_string(),
                quest_id: quest_id.clone(),
            });
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::load_vendored;
    use crate::route_dsl::parse_route_file;
    use crate::vendor::read_act_route;
    use std::collections::BTreeSet;

    #[test]
    fn act1_league_start_golden_contexts() {
        let (areas, quests) = load_vendored().unwrap();
        let defines: BTreeSet<String> = ["LEAGUE_START".to_string()].into_iter().collect();
        let sections = parse_route_file(&read_act_route(1).unwrap(), &defines).unwrap();

        let mut state = WalkState::campaign_start();
        let steps = walk_act(1, &sections, &areas, &quests, &mut state).unwrap();

        let contexts: Vec<&str> = steps
            .iter()
            .take(12)
            .map(|s| s.area_context.as_str())
            .collect();
        assert_eq!(
            contexts,
            vec![
                "1_1_1",    // kill Hillock (Twilight Strand)
                "1_1_1",    // enter Lioneye's Watch
                "1_1_town", // hand in Enemy at the Gate
                "1_1_town", // enter The Coast
                "1_1_2",    // get waypoint (league start)
                "1_1_2",    // enter The Mud Flats
                "1_1_3",    // find 3x Glyph
                "1_1_3",    // enter The Submerged Passage
                "1_1_4_1",  // waypoint to The Coast
                "1_1_2",    // enter The Tidal Island
                "1_1_2a",   // kill Hailrake
                "1_1_2a",   // logout
            ]
        );

        assert_eq!(steps[0].id, "a1-s001");
        assert_eq!(steps[0].act, 1);
    }

    /// Mirrors exile-leveling's EvaluateQuest (fragment/index.ts): a
    /// `{quest|id}` hand-in with no explicit reward offer ids means "collect
    /// every reward offer of that quest now", so the compiled step must carry
    /// all offer ids from quests.json. Explicit ids stay as written.
    #[test]
    fn quest_without_explicit_offers_expands_to_all_reward_offers() {
        let (areas, quests) = load_vendored().unwrap();
        let defines: BTreeSet<String> = ["LEAGUE_START".to_string()].into_iter().collect();
        let sections = parse_route_file(&read_act_route(1).unwrap(), &defines).unwrap();

        let mut state = WalkState::campaign_start();
        let steps = walk_act(1, &sections, &areas, &quests, &mut state).unwrap();

        let offers_for = |quest_id: &str| -> Vec<Vec<String>> {
            steps
                .iter()
                .flat_map(|s| &s.fragments)
                .filter_map(|f| match f {
                    Fragment::Quest {
                        quest_id: id,
                        reward_offer_ids,
                    } if id == quest_id => Some(reward_offer_ids.clone()),
                    _ => None,
                })
                .collect()
        };

        // a1q4 (Breaking Some Eggs) is handed in without explicit offers;
        // quests.json lists two reward offers (a1q4, a1q4b).
        for offers in offers_for("a1q4") {
            assert_eq!(offers, vec!["a1q4".to_string(), "a1q4b".to_string()]);
        }
        assert!(!offers_for("a1q4").is_empty());

        // a1q2 (The Caged Brute) hand-ins name explicit offers; they must be
        // preserved verbatim, not expanded.
        let a1q2 = offers_for("a1q2");
        assert!(a1q2.iter().all(|o| o.len() == 1));
        assert!(a1q2.contains(&vec!["a1q2b".to_string()]));
    }

    #[test]
    fn walk_validates_area_ids() {
        let (areas, quests) = load_vendored().unwrap();
        let sections = parse_route_file("➞ {enter|not_a_real_area}\n", &BTreeSet::new()).unwrap();
        let mut state = WalkState::campaign_start();
        let err = walk_act(1, &sections, &areas, &quests, &mut state).unwrap_err();
        assert!(matches!(err, WalkError::UnknownArea { .. }));
    }

    #[test]
    fn full_campaign_walks_without_errors_in_both_variants() {
        let (areas, quests) = load_vendored().unwrap();
        for defines in [
            BTreeSet::new(),
            ["LEAGUE_START".to_string()]
                .into_iter()
                .collect::<BTreeSet<_>>(),
        ] {
            let mut state = WalkState::campaign_start();
            for act in 1..=10u8 {
                let sections = parse_route_file(&read_act_route(act).unwrap(), &defines).unwrap();
                walk_act(act, &sections, &areas, &quests, &mut state)
                    .unwrap_or_else(|e| panic!("act {act} walk failed: {e}"));
            }
        }
    }
}
