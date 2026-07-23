//! Overlay composition: assembles a display-ready `OverlayModel` from
//! `RouteEngine`/`TaskEngine` state plus static content (areas, layouts).
//! Pure — no I/O.

use std::collections::BTreeMap;

use content::game_data::AreaMap;
use content::layouts::{AuditStatus, LayoutEntry};
use route_engine::RouteEngine;
use serde::Serialize;
use task_engine::TaskEngine;

pub mod render;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteView {
    pub text: String,
    pub stale: bool,
}

/// A layout diagram image. `file` resolves against the content pack's flat
/// `assets/` directory (see `content::layouts` / the poelayouts extractor).
/// `stale` mirrors the image's audit status (`Outdated`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageView {
    pub file: String,
    pub stale: bool,
}

/// Build-plan context threaded into `compose` so it can surface leveling
/// milestones as reminders while in town. `None` when no build has been
/// imported (or the current session predates build import).
pub struct BuildContext<'a> {
    pub plan: &'a pob_import::LevelingBuildPlan,
    pub player_level: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayModel {
    pub zone_name: String,
    pub area_id: String,
    pub act: u8,
    pub off_route_zone: Option<String>,
    /// Layout diagram images for the active zone. Files resolve against the
    /// content pack's flat `assets/` directory.
    pub layout_images: Vec<ImageView>,
    pub layout_notes: Vec<NoteView>,
    pub steps_in_zone: Vec<String>,
    /// Rendered `#sub` hint lines for the active group's steps.
    pub sub_hints: Vec<String>,
    pub primary: String,
    pub next_zone: Option<String>,
    pub pending_count: usize,
    pub town_reminders: Vec<String>,
    /// Leveling-build milestones due soon, shown only while in town. See
    /// `build_reminders` (the free function) for the level-window rule.
    pub build_reminders: Vec<String>,
    /// Whether the active area is a town (always false once the route is
    /// complete).
    pub is_town: bool,
    pub route_complete: bool,
}

/// Index layout entries by area id for `compose` lookups.
pub fn layouts_by_area(entries: Vec<LayoutEntry>) -> BTreeMap<String, LayoutEntry> {
    entries
        .into_iter()
        .map(|e| (e.area_id.clone(), e))
        .collect()
}

/// Map a layout entry's descriptions + notes to display-ready `NoteView`s,
/// per the audit-status rules: `Corrected` uses the correction text (or the
/// original if no correction is recorded) and is not stale; `Outdated` uses
/// the original text and IS stale; `Unaudited`/`Verified` use the original
/// text and are not stale.
fn note_views(entry: &LayoutEntry) -> Vec<NoteView> {
    entry
        .descriptions
        .iter()
        .chain(entry.notes.iter())
        .map(|item| {
            let (text, stale) = match item.audit.status {
                AuditStatus::Corrected => (
                    item.audit
                        .correction
                        .clone()
                        .unwrap_or_else(|| item.text.clone()),
                    false,
                ),
                AuditStatus::Outdated => (item.text.clone(), true),
                AuditStatus::Unaudited | AuditStatus::Verified => (item.text.clone(), false),
            };
            NoteView { text, stale }
        })
        .collect()
}

/// Look up an area's display name, falling back to the raw id when the
/// area is unknown.
fn display_name<'a>(areas: &'a AreaMap, area_id: &'a str) -> String {
    areas
        .get(area_id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| area_id.to_string())
}

pub fn compose(
    engine: &RouteEngine,
    tasks: &TaskEngine,
    layouts: &BTreeMap<String, LayoutEntry>,
    areas: &AreaMap,
    build: Option<BuildContext<'_>>,
) -> OverlayModel {
    let route_complete = engine.is_complete();
    let active_area_id = engine.active_area();

    let area_id = active_area_id.unwrap_or("").to_string();
    let act = if route_complete {
        0
    } else {
        engine.act().unwrap_or(0)
    };
    let zone_name = if route_complete {
        "Campaign complete".to_string()
    } else {
        display_name(areas, &area_id)
    };

    let off_route_zone = engine.off_route().map(|id| display_name(areas, id));

    let layout_entry = if route_complete {
        None
    } else {
        layouts.get(&area_id)
    };
    let layout_images: Vec<ImageView> = layout_entry
        .map(|e| {
            e.images
                .iter()
                .map(|i| ImageView {
                    file: i.file.clone(),
                    stale: i.audit.status == AuditStatus::Outdated,
                })
                .collect()
        })
        .unwrap_or_default();
    let layout_notes = layout_entry.map(note_views).unwrap_or_default();

    let steps_in_zone: Vec<String> = engine
        .active_steps()
        .iter()
        .map(|s| render::render_fragments(&s.fragments, areas))
        .filter(|s| !s.is_empty())
        .collect();

    let sub_hints: Vec<String> = engine
        .active_steps()
        .iter()
        .flat_map(|s| s.sub_steps.iter())
        .map(|frags| render::render_fragments(frags, areas))
        .filter(|s| !s.is_empty())
        .collect();

    let next_zone = engine
        .next_transition_area()
        .map(|id| display_name(areas, id));

    let primary = if route_complete {
        String::new()
    } else if let Some(first) = steps_in_zone.first() {
        first.clone()
    } else if let Some(nz) = &next_zone {
        format!("Continue to {nz}")
    } else {
        String::new()
    };

    let is_town = !route_complete && areas.get(&area_id).map(|a| a.is_town_area).unwrap_or(false);
    let town_reminders: Vec<String> = if is_town {
        tasks
            .town_reminders()
            .iter()
            .map(|p| p.label.clone())
            .collect()
    } else {
        vec![]
    };

    let build_reminders = build_reminders_for(is_town, build.as_ref());

    OverlayModel {
        zone_name,
        area_id,
        act,
        off_route_zone,
        layout_images,
        layout_notes,
        steps_in_zone,
        sub_hints,
        primary,
        next_zone,
        pending_count: tasks.pending_count(),
        town_reminders,
        build_reminders,
        is_town,
        route_complete,
    }
}

/// Leveling-build milestones due soon: shown only while in town, with a
/// known player level, for a plan whose reliability is not `Unsupported`.
/// A milestone `m` is shown when `m.level <= player_level + 2 &&
/// m.level + 5 >= player_level` — i.e. it's coming up within the next two
/// levels or hasn't aged out more than five levels back.
///
/// Capped at 4: when more than 4 milestones fall in the window, keep the 4
/// CLOSEST to (or above) the player's level rather than the 4 lowest —
/// sorting ascending by level and taking the last 4 does exactly that,
/// since the window is already level-bounded on both sides. The result
/// stays sorted ascending for display (it's a suffix of an ascending sort,
/// so no re-sort is needed).
fn build_reminders_for(is_town: bool, build: Option<&BuildContext<'_>>) -> Vec<String> {
    let Some(build) = build else {
        return vec![];
    };
    let Some(player_level) = build.player_level else {
        return vec![];
    };
    if !is_town || build.plan.reliability == pob_import::Reliability::Unsupported {
        return vec![];
    }

    let mut due: Vec<&pob_import::Milestone> = build
        .plan
        .milestones
        .iter()
        .filter(|m| {
            m.level <= player_level.saturating_add(2) && m.level.saturating_add(5) >= player_level
        })
        .collect();
    due.sort_by_key(|m| m.level);
    let keep_from = due.len().saturating_sub(4);
    due[keep_from..].iter().map(|m| m.label.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::compile::{Variant, compile_route_pack};
    use content::game_data::load_vendored;
    use content::layouts::load_all_layouts;
    use route_engine::RouteEngine;
    use task_engine::TaskEngine;

    fn fixture() -> (
        RouteEngine,
        TaskEngine,
        std::collections::BTreeMap<String, content::layouts::LayoutEntry>,
        content::game_data::AreaMap,
    ) {
        let (areas, _) = load_vendored().unwrap();
        let pack = compile_route_pack(Variant::LeagueStart).unwrap();
        let engine = RouteEngine::from_pack(&pack, areas.clone());
        let layouts = layouts_by_area(load_all_layouts().unwrap());
        (engine, TaskEngine::new(areas.clone()), layouts, areas)
    }

    #[test]
    fn composes_coast_state_with_real_layout_content() {
        let (mut engine, tasks, layouts, areas) = fixture();
        engine.on_area_entered("1_1_town");
        engine.on_area_entered("1_1_2");

        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert_eq!(m.zone_name, "The Coast");
        assert_eq!(m.area_id, "1_1_2");
        assert_eq!(m.act, 1);
        assert!(!m.route_complete);
        assert_eq!(m.off_route_zone, None);
        assert!(!m.layout_images.is_empty(), "The Coast has layout images");
        assert!(!m.layout_notes.is_empty());
        assert!(
            m.layout_notes.iter().all(|n| !n.stale),
            "all content is unaudited, not stale"
        );
        assert!(!m.steps_in_zone.is_empty());
        assert_eq!(m.primary, m.steps_in_zone[0]);
        assert!(m.next_zone.is_some());
        assert!(m.town_reminders.is_empty(), "not in town");
    }

    #[test]
    fn off_route_zone_is_reported_with_display_name() {
        let (mut engine, tasks, layouts, areas) = fixture();
        engine.on_area_entered("1_1_town");
        engine.on_area_entered("1_1_2");
        engine.on_area_entered("1_1_town"); // off-route
        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert_eq!(m.off_route_zone.as_deref(), Some("Lioneye's Watch"));
        assert_eq!(m.zone_name, "The Coast"); // progress display unchanged
    }

    #[test]
    fn town_reminders_only_in_town() {
        let (mut engine, mut tasks, layouts, areas) = fixture();
        // Manufacture a skipped reward pending.
        let s = content::walk::CompiledStep {
            id: "a1-s099".into(),
            act: 1,
            section: "Act 1".into(),
            area_context: "1_1_town".into(),
            fragments: vec![content::route_dsl::Fragment::RewardQuest {
                item: "Quicksilver Flask".into(),
            }],
            sub_steps: vec![],
        };
        tasks.on_step_passed(&s, route_engine::StepStatus::Skipped);

        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert!(m.town_reminders.is_empty(), "Twilight Strand is not a town");
        assert!(!m.is_town);
        assert_eq!(m.pending_count, 1);

        engine.on_area_entered("1_1_town");
        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert!(m.is_town);
        assert_eq!(
            m.town_reminders,
            vec!["Claim quest reward: Quicksilver Flask".to_string()]
        );
    }

    #[test]
    fn route_complete_zeroing() {
        // Feed every group's area_context in order (mirrors route_engine's
        // full-replay test) to drive the engine to completion, then assert
        // the composed overlay is fully zeroed out.
        let (mut engine, tasks, layouts, areas) = fixture();
        let contexts: Vec<String> = {
            let mut cs = Vec::new();
            for s in engine.steps() {
                if cs.last() != Some(&s.area_context) {
                    cs.push(s.area_context.clone());
                }
            }
            cs
        };
        for c in &contexts {
            engine.on_area_entered(c);
        }
        assert!(engine.is_complete());

        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert_eq!(m.zone_name, "Campaign complete");
        assert_eq!(m.act, 0);
        assert_eq!(m.area_id, "");
        assert_eq!(m.primary, "");
        assert!(m.next_zone.is_none());
        assert!(m.route_complete);
        assert!(!m.is_town);
        assert!(m.layout_images.is_empty());
        assert!(m.steps_in_zone.is_empty());
    }

    #[test]
    fn build_reminders_are_town_gated_and_level_windowed() {
        let (mut engine, tasks, layouts, areas) = fixture();
        let plan = pob_import::LevelingBuildPlan {
            class_name: "Ranger".into(),
            ascend_name: None,
            skill_sets: vec![],
            passive_spec_titles: vec![],
            notes: None,
            milestones: vec![
                pob_import::Milestone {
                    level: 4,
                    label: "Gem available: Frostblink".into(),
                    reliability: pob_import::Reliability::Structured,
                },
                pob_import::Milestone {
                    level: 12,
                    label: "Gem available: Toxic Rain".into(),
                    reliability: pob_import::Reliability::Structured,
                },
                pob_import::Milestone {
                    level: 2,
                    label: "Old".into(),
                    reliability: pob_import::Reliability::Structured,
                },
            ],
            reliability: pob_import::Reliability::Structured,
        };
        let ctx = |lvl| {
            Some(BuildContext {
                plan: &plan,
                player_level: Some(lvl),
            })
        };

        // Not in town: no reminders even with a build.
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(5));
        assert!(m.build_reminders.is_empty());

        engine.on_area_entered("1_1_town");
        // Level 5: window is m.level <= 7 && m.level + 5 >= 5.
        // Frostblink (4): 4<=7 && 9>=5 -> shown. Old (2): 2<=7 && 7>=5 -> ALSO
        // shown (not aged out at level 5). Toxic Rain (12): 12<=7 is false ->
        // not shown. Sorted by level: Old (2) before Frostblink (4).
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(5));
        assert_eq!(
            m.build_reminders,
            vec!["Old".to_string(), "Gem available: Frostblink".to_string()]
        );
        // Level 10: Toxic Rain (12) now within +2 lookahead (12<=12 && 17>=10);
        // Frostblink (4+5=9 < 10) and Old (2+5=7 < 10) have aged out.
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(10));
        assert_eq!(
            m.build_reminders,
            vec!["Gem available: Toxic Rain".to_string()]
        );
        // No player level: nothing.
        let m = compose(
            &engine,
            &tasks,
            &layouts,
            &areas,
            Some(BuildContext {
                plan: &plan,
                player_level: None,
            }),
        );
        assert!(m.build_reminders.is_empty());
        // No build: nothing (and everything else unchanged).
        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert!(m.build_reminders.is_empty());
    }

    #[test]
    fn build_reminders_cap_keeps_closest_upcoming_when_over_four() {
        let (mut engine, tasks, layouts, areas) = fixture();
        let milestone = |level: u16, label: &str| pob_import::Milestone {
            level,
            label: label.to_string(),
            reliability: pob_import::Reliability::Structured,
        };
        // 6 milestones, levels 2..7, all Structured.
        let plan = pob_import::LevelingBuildPlan {
            class_name: "Ranger".into(),
            ascend_name: None,
            skill_sets: vec![],
            passive_spec_titles: vec![],
            notes: None,
            milestones: vec![
                milestone(2, "A"),
                milestone(3, "B"),
                milestone(4, "C"),
                milestone(5, "D"),
                milestone(6, "E"),
                milestone(7, "F"),
            ],
            reliability: pob_import::Reliability::Structured,
        };
        engine.on_area_entered("1_1_town");

        // Level 7: window is m.level <= 9 && m.level + 5 >= 7, i.e.
        // m.level >= 2 — all 6 milestones (levels 2..7) fall in the window.
        // Old rule (lowest 4) would show A,B,C,D (levels 2-5). New rule
        // (closest-to-or-above player level, i.e. highest 4) shows C,D,E,F
        // (levels 4-7), still ascending.
        let ctx = Some(BuildContext {
            plan: &plan,
            player_level: Some(7),
        });
        let m = compose(&engine, &tasks, &layouts, &areas, ctx);
        assert_eq!(
            m.build_reminders,
            vec![
                "C".to_string(),
                "D".to_string(),
                "E".to_string(),
                "F".to_string()
            ]
        );
    }

    /// A crafted PoB (e.g. a skill-set title like "65531-65535") could
    /// produce a milestone level near `u16::MAX`. Plain `m.level + 5` on a
    /// milestone at 65535 would overflow and panic in a debug build; this
    /// proves the `saturating_add` fix in `build_reminders_for` holds even
    /// under debug overflow checks, and that the result stays sensible
    /// (the far-future milestone simply doesn't show for a level-90
    /// player).
    #[test]
    fn build_reminders_for_does_not_panic_on_near_u16_max_milestone_level() {
        let (mut engine, tasks, layouts, areas) = fixture();
        let plan = pob_import::LevelingBuildPlan {
            class_name: "Ranger".into(),
            ascend_name: None,
            skill_sets: vec![],
            passive_spec_titles: vec![],
            notes: None,
            milestones: vec![
                pob_import::Milestone {
                    level: 65535,
                    label: "Overflow bait".into(),
                    reliability: pob_import::Reliability::Structured,
                },
                pob_import::Milestone {
                    level: 90,
                    label: "Gem available: Barrage".into(),
                    reliability: pob_import::Reliability::Structured,
                },
            ],
            reliability: pob_import::Reliability::Structured,
        };
        engine.on_area_entered("1_1_town");
        let ctx = Some(BuildContext {
            plan: &plan,
            player_level: Some(90),
        });
        let m = compose(&engine, &tasks, &layouts, &areas, ctx);
        // The near-u16::MAX milestone is nowhere near level 90 and must not
        // appear (and, above all, must not have panicked getting here); the
        // in-window milestone still shows.
        assert_eq!(
            m.build_reminders,
            vec!["Gem available: Barrage".to_string()]
        );
    }
}
