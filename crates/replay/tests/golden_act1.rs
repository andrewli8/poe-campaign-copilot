use replay::{fixtures_dir, replay_fixture};
use session::SessionEvent;

#[test]
fn act1_opening_golden_sequence() {
    let events = replay_fixture(&fixtures_dir().join("act1-opening.log")).unwrap();

    let compact: Vec<String> = events
        .iter()
        .map(|e| match e {
            SessionEvent::SessionStarted { .. } => "start".to_string(),
            SessionEvent::AreaEntered {
                area_id,
                is_town,
                new_instance,
                ..
            } => format!(
                "enter:{area_id}:{}:{}",
                if *is_town { "town" } else { "field" },
                if *new_instance { "new" } else { "revisit" }
            ),
            SessionEvent::LevelUp { level, .. } => format!("level:{level}"),
            SessionEvent::Slain { .. } => "slain".to_string(),
            SessionEvent::UnresolvedArea { display_name, .. } => {
                format!("unresolved:{display_name}")
            }
        })
        .collect();

    assert_eq!(
        compact,
        vec![
            "start",
            "enter:1_1_1:field:new",
            "level:2",
            "enter:1_1_town:town:new",
            "enter:1_1_2:field:new",
            "level:3",
            "enter:1_1_town:town:revisit",
            "enter:1_1_2:field:revisit",
            "enter:1_1_3:field:new",
            "slain",
            "enter:1_1_3:field:revisit",
        ]
    );
}
