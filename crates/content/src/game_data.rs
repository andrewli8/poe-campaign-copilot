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
    #[serde(default)]
    pub level: Option<u8>,
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
    #[serde(deserialize_with = "act_from_string")]
    pub act: u8,
    #[serde(default)]
    pub reward_offers: BTreeMap<String, RewardOffer>,
}

/// One reward screen of a quest: gems offered as free quest rewards and gems
/// that become purchasable from a vendor once the quest is complete.
#[derive(Debug, Clone, Deserialize)]
pub struct RewardOffer {
    #[serde(default)]
    pub quest_npc: Option<String>,
    #[serde(default)]
    pub quest: BTreeMap<String, QuestRewardItem>,
    #[serde(default)]
    pub vendor: BTreeMap<String, VendorRewardItem>,
}

/// A quest-reward item offer. An empty `classes` list means the offer is
/// unrestricted (available to every class).
#[derive(Debug, Clone, Deserialize)]
pub struct QuestRewardItem {
    #[serde(default)]
    pub classes: Vec<String>,
}

/// A vendor item offer. An empty `classes` list means the offer is
/// unrestricted (available to every class).
#[derive(Debug, Clone, Deserialize)]
pub struct VendorRewardItem {
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub npc: Option<String>,
}

/// quests.json stores act as a string (e.g. `"act": "1"`).
fn act_from_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let raw = String::deserialize(deserializer)?;
    raw.parse().map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Gem {
    pub id: String,
    pub name: String,
    pub required_level: u8,
    pub is_support: bool,
}

pub type AreaMap = BTreeMap<String, Area>;
pub type QuestMap = BTreeMap<String, Quest>;
pub type GemMap = BTreeMap<String, Gem>;

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

pub fn load_gems(json: &str) -> Result<GemMap, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn load_vendored() -> Result<(AreaMap, QuestMap), GameDataError> {
    let data = vendor_dir().join("data");
    let areas = load_areas(&std::fs::read_to_string(data.join("areas.json"))?)?;
    let quests = load_quests(&std::fs::read_to_string(data.join("quests.json"))?)?;
    Ok((areas, quests))
}

pub fn load_vendored_gems() -> Result<GemMap, GameDataError> {
    let data = vendor_dir().join("data");
    let json = std::fs::read_to_string(data.join("gems.json"))?;
    load_gems(&json).map_err(GameDataError::Json)
}

pub fn gems_by_name(gems: &GemMap) -> BTreeMap<String, Gem> {
    let mut result = BTreeMap::new();
    for gem in gems.values() {
        // Royale variants share names with campaign gems but often carry
        // lower required levels; they are not obtainable in the campaign,
        // so they must never win the by-name dedupe.
        if gem.id.contains("Royale") {
            continue;
        }
        result
            .entry(gem.name.clone())
            .and_modify(|existing: &mut Gem| {
                if gem.required_level < existing.required_level {
                    *existing = gem.clone();
                }
            })
            .or_insert_with(|| gem.clone());
    }
    result
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
        assert_eq!(q.act, 1);
        let offer = q.reward_offers.get("a1q1").expect("a1q1 reward offer");
        assert_eq!(offer.quest_npc.as_deref(), Some("Tarkleigh"));
        let fp = offer
            .quest
            .get("Metadata/Items/Gems/SkillGemFreezingPulse")
            .expect("Freezing Pulse quest reward");
        assert!(fp.classes.contains(&"Witch".to_string()));
        let siosa = quests.get("a3q12").expect("A Fixture of Fate exists");
        let siosa_offer = siosa.reward_offers.get("a3q12").expect("Siosa offer");
        let siosa_fp = siosa_offer
            .vendor
            .get("Metadata/Items/Gems/SkillGemFreezingPulse")
            .expect("Siosa sells Freezing Pulse");
        assert!(siosa_fp.classes.is_empty(), "Siosa offers are unrestricted");
        assert_eq!(siosa_fp.npc.as_deref(), Some("Siosa"));

        assert!(areas.len() > 100);
        assert!(quests.len() > 40);
    }

    #[test]
    fn loads_vendored_gems() {
        let gems = load_vendored_gems().expect("gems load");
        assert!(gems.len() > 500);
        let by_name = gems_by_name(&gems);

        // Test single gem without duplicates
        let fb = by_name.get("Frostblink").expect("Frostblink exists");
        assert_eq!(fb.required_level, 4);
        assert!(!fb.is_support);

        // Distinct names are separate
        assert!(by_name.contains_key("Fireball"));

        // Royale variants share names but are not obtainable in the
        // campaign; they must never win the by-name dedupe. Ice Nova's
        // campaign gem requires level 12 (the Royale variant requires 4).
        let ice_nova = by_name.get("Ice Nova").expect("Ice Nova exists");
        assert_eq!(ice_nova.required_level, 12);

        let leap_slam = by_name.get("Leap Slam").expect("Leap Slam exists");
        assert_eq!(leap_slam.required_level, 10);

        let toxic_rain = by_name.get("Toxic Rain").expect("Toxic Rain exists");
        assert_eq!(toxic_rain.required_level, 12);

        assert!(by_name.values().all(|g| !g.id.contains("Royale")));
    }
}
