//! End-to-end pilot: golden Client.txt fixture -> session -> route/task
//! engines -> composer, asserting the overlay state at each transition.

use composer::{compose, layouts_by_area};
use content::compile::{Variant, compile_route_pack};
use content::game_data::load_vendored;
use content::layouts::load_all_layouts;
use route_engine::RouteEngine;
use session::{SessionEvent, SessionTracker};
use task_engine::TaskEngine;

#[test]
fn fixture_drives_engines_to_expected_overlay_states() {
    let (areas, _) = load_vendored().unwrap();
    let pack = compile_route_pack(Variant::LeagueStart).unwrap();
    let mut engine = RouteEngine::from_pack(&pack, areas.clone());
    let mut tasks = TaskEngine::new();
    let layouts = layouts_by_area(load_all_layouts().unwrap());
    let mut tracker = SessionTracker::new(areas.clone());

    let text = std::fs::read_to_string(replay::fixtures_dir().join("act1-opening.log")).unwrap();

    let mut observed = Vec::new();
    for line in text.lines() {
        for ev in tracker.on_raw(&event_parser::parse_line(line)) {
            if let SessionEvent::AreaEntered { area_id, .. } = &ev {
                let advance = engine.on_area_entered(area_id);
                for &i in advance.newly_done.iter().chain(&advance.newly_skipped) {
                    let status = engine.statuses()[i];
                    let step = engine.steps()[i].clone();
                    tasks.on_step_passed(&step, status);
                }
                let m = compose(&engine, &tasks, &layouts, &areas);
                observed.push((
                    area_id.clone(),
                    format!("{:?}", advance.kind),
                    m.zone_name.clone(),
                    m.off_route_zone.is_some(),
                ));
            }
        }
    }

    let compact: Vec<(&str, &str, &str, bool)> = observed
        .iter()
        .map(|(a, k, z, o)| (a.as_str(), k.as_str(), z.as_str(), *o))
        .collect();

    assert_eq!(
        compact,
        vec![
            ("1_1_1", "InPlace", "The Twilight Strand", false),
            ("1_1_town", "Advanced", "Lioneye's Watch", false),
            ("1_1_2", "Advanced", "The Coast", false),
            // Early town revisit: off-route, progress display stays on Coast.
            (
                "1_1_town",
                "OffRoute { area_id: \"1_1_town\" }",
                "The Coast",
                true
            ),
            ("1_1_2", "Resumed", "The Coast", false),
            ("1_1_3", "Advanced", "The Mud Flats", false),
            // Death + same-instance re-entry: idempotent.
            ("1_1_3", "InPlace", "The Mud Flats", false),
        ]
    );

    // Recompose at the end: active zone is The Mud Flats, which carried
    // real layout content when active.
    let m = compose(&engine, &tasks, &layouts, &areas);
    assert_eq!(m.zone_name, "The Mud Flats");
    assert!(!m.layout_images.is_empty(), "Mud Flats has layout images");
}
