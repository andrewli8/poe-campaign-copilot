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
        is_town,
        route_complete,
    }
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

        let m = compose(&engine, &tasks, &layouts, &areas);
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
        let m = compose(&engine, &tasks, &layouts, &areas);
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

        let m = compose(&engine, &tasks, &layouts, &areas);
        assert!(m.town_reminders.is_empty(), "Twilight Strand is not a town");
        assert!(!m.is_town);
        assert_eq!(m.pending_count, 1);

        engine.on_area_entered("1_1_town");
        let m = compose(&engine, &tasks, &layouts, &areas);
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

        let m = compose(&engine, &tasks, &layouts, &areas);
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
}
