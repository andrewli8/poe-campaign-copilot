//! Fragment rendering: turns compiled route DSL fragments into
//! human-readable step text.

use content::game_data::AreaMap;
use content::route_dsl::Fragment;

pub const DIR_GLYPHS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];

pub fn render_fragments(frags: &[Fragment], areas: &AreaMap) -> String {
    let mut out = String::new();
    for f in frags {
        match f {
            Fragment::Text { value } => out.push_str(value),
            Fragment::Kill { name } | Fragment::Arena { name } => out.push_str(name),
            Fragment::Area { area_id }
            | Fragment::Enter { area_id }
            | Fragment::WaypointUse { area_id } => {
                out.push_str(
                    areas
                        .get(area_id)
                        .map(|a| a.name.as_str())
                        .unwrap_or(area_id),
                );
            }
            Fragment::Logout => out.push_str("log out"),
            Fragment::Waypoint => out.push_str("the waypoint"),
            Fragment::WaypointGet => out.push_str("waypoint"),
            Fragment::PortalSet => out.push_str("place a portal"),
            Fragment::PortalUse => out.push_str("take the portal"),
            Fragment::Quest { .. } => out.push_str("hand in quest"),
            Fragment::QuestText { text } | Fragment::Generic { text } | Fragment::Copy { text } => {
                out.push_str(text)
            }
            Fragment::RewardQuest { item } => out.push_str(item),
            Fragment::RewardVendor { item, .. } => {
                out.push_str("buy ");
                out.push_str(item);
            }
            Fragment::Trial => out.push_str("Trial of Ascendancy"),
            Fragment::Ascend { .. } => out.push_str("the Labyrinth"),
            Fragment::Crafting { .. } => out.push_str("crafting recipe"),
            Fragment::Dir { dir_index } => {
                out.push_str(DIR_GLYPHS.get(*dir_index as usize).copied().unwrap_or("?"))
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::game_data::load_vendored;
    use content::route_dsl::Fragment;

    #[test]
    fn renders_enter_with_area_name_and_dir_glyph() {
        let (areas, _) = load_vendored().unwrap();
        let s = render_fragments(
            &[
                Fragment::Text {
                    value: "➞ ".into()
                },
                Fragment::Enter {
                    area_id: "1_1_2".into(),
                },
                Fragment::Text {
                    value: " then ".into(),
                },
                Fragment::Dir { dir_index: 1 },
            ],
            &areas,
        );
        assert_eq!(s, "➞ The Coast then ↗");
    }

    #[test]
    fn renders_actions_and_unknown_area_fallback() {
        let (areas, _) = load_vendored().unwrap();
        let s = render_fragments(
            &[
                Fragment::Kill {
                    name: "Hillock".into(),
                },
                Fragment::Text { value: ", ".into() },
                Fragment::PortalSet,
                Fragment::Text { value: ", ".into() },
                Fragment::Enter {
                    area_id: "bogus".into(),
                },
            ],
            &areas,
        );
        assert_eq!(s, "Hillock, place a portal, bogus");
    }
}
