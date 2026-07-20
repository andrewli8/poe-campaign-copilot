//! Typed access to vendored exile-leveling game data.

use std::collections::BTreeMap;

use serde::Deserialize;
use thiserror::Error;

use crate::vendor::vendor_dir;

#[derive(Debug, Clone, Deserialize)]
pub struct Area {
    pub id: String,
    pub name: String,
    pub act: u8,
    pub has_waypoint: bool,
    pub is_town_area: bool,
    pub parent_town_area_id: Option<String>,
    #[serde(default)]
    pub connection_ids: Vec<String>,
    #[serde(default)]
    pub crafting_recipes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Quest {
    pub id: String,
    pub name: String,
}

pub type AreaMap = BTreeMap<String, Area>;
pub type QuestMap = BTreeMap<String, Quest>;

#[derive(Debug, Error)]
pub enum GameDataError {
    #[error("failed to read vendored game data: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse vendored game data: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn load_areas(json: &str) -> Result<AreaMap, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn load_quests(json: &str) -> Result<QuestMap, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn load_vendored() -> Result<(AreaMap, QuestMap), GameDataError> {
    let data = vendor_dir().join("data");
    let areas = load_areas(&std::fs::read_to_string(data.join("areas.json"))?)?;
    let quests = load_quests(&std::fs::read_to_string(data.join("quests.json"))?)?;
    Ok((areas, quests))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_vendored_areas_and_quests() {
        let (areas, quests) = load_vendored().expect("vendored data loads");

        let strand = areas.get("1_1_1").expect("Twilight Strand exists");
        assert_eq!(strand.name, "The Twilight Strand");
        assert_eq!(strand.act, 1);
        assert!(!strand.is_town_area);
        assert_eq!(strand.parent_town_area_id.as_deref(), Some("1_1_town"));
        assert!(strand.connection_ids.contains(&"1_1_town".to_string()));

        let town = areas.get("1_1_town").expect("Lioneye's Watch exists");
        assert!(town.is_town_area);
        assert!(town.has_waypoint);

        let q = quests.get("a1q1").expect("Enemy at the Gate exists");
        assert_eq!(q.name, "Enemy at the Gate");

        assert!(areas.len() > 100);
        assert!(quests.len() > 40);
    }
}
