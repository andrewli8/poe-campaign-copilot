use std::collections::BTreeSet;

use content::route_dsl::parse_route_file;
use content::vendor::read_act_route;

fn league_start_defines() -> BTreeSet<String> {
    ["LEAGUE_START".to_string()].into_iter().collect()
}

#[test]
fn all_acts_parse_in_both_variants() {
    for defines in [BTreeSet::new(), league_start_defines()] {
        for act in 1..=10u8 {
            let source = read_act_route(act)
                .unwrap_or_else(|e| panic!("missing vendored route for act {act}: {e}"));
            let sections = parse_route_file(&source, &defines)
                .unwrap_or_else(|e| panic!("act {act} failed to parse: {e}"));
            let step_count: usize = sections.iter().map(|s| s.steps.len()).sum();
            assert!(
                step_count > 10,
                "act {act}: suspiciously few steps ({step_count})"
            );
        }
    }
}
