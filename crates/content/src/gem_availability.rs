//! Class-aware gem availability derived from vendored quest reward data.
//!
//! `quests.json` records, per quest, which gems are offered as quest rewards
//! and which become purchasable from a vendor, each with the list of classes
//! the offer applies to (an empty list means every class). This module folds
//! those offers into "earliest point a given class can obtain each gem".

use std::collections::BTreeMap;

use crate::game_data::{AreaMap, QuestMap};

/// How a gem is obtained at its earliest availability point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvailabilitySource {
    /// Free pick from the quest's reward screen.
    QuestReward,
    /// Purchasable from an NPC once the quest is complete.
    Vendor,
}

/// The earliest point a class can obtain a gem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GemAvailability {
    pub act: u8,
    pub quest_id: String,
    pub quest_name: String,
    /// Vendor NPC name for `Vendor` offers; quest turn-in NPC otherwise.
    pub npc: Option<String>,
    pub source: AvailabilitySource,
}

/// Earliest availability of every gem for `class_name`, keyed by gem id
/// (e.g. `Metadata/Items/Gems/SkillGemToxicRain`).
///
/// An offer applies to the class when its class list is empty (unrestricted,
/// e.g. Siosa in act 3 and Lilly Roth from act 6) or contains `class_name`.
/// Earlier acts win; within an act a quest reward beats a vendor offer, then
/// the lexicographically smaller quest id wins for determinism.
pub fn availability_for_class(
    quests: &QuestMap,
    class_name: &str,
) -> BTreeMap<String, GemAvailability> {
    let mut earliest: BTreeMap<String, GemAvailability> = BTreeMap::new();

    for quest in quests.values() {
        for offer in quest.reward_offers.values() {
            let quest_rewards = offer
                .quest
                .iter()
                .map(|(gem_id, reward)| (gem_id, &reward.classes, offer.quest_npc.clone()))
                .map(|(gem_id, classes, npc)| {
                    (gem_id, classes, npc, AvailabilitySource::QuestReward)
                });
            let vendor_rewards = offer
                .vendor
                .iter()
                .map(|(gem_id, reward)| (gem_id, &reward.classes, reward.npc.clone()))
                .map(|(gem_id, classes, npc)| (gem_id, classes, npc, AvailabilitySource::Vendor));

            for (gem_id, classes, npc, source) in quest_rewards.chain(vendor_rewards) {
                if !gem_id.contains("/Gems/") {
                    continue;
                }
                if !classes.is_empty() && !classes.iter().any(|c| c == class_name) {
                    continue;
                }
                let candidate = GemAvailability {
                    act: quest.act,
                    quest_id: quest.id.clone(),
                    quest_name: quest.name.clone(),
                    npc,
                    source,
                };
                match earliest.get(gem_id) {
                    Some(existing) if !beats(&candidate, existing) => {}
                    _ => {
                        earliest.insert(gem_id.clone(), candidate);
                    }
                }
            }
        }
    }

    earliest
}

fn beats(candidate: &GemAvailability, existing: &GemAvailability) -> bool {
    (candidate.act, candidate.source, &candidate.quest_id)
        < (existing.act, existing.source, &existing.quest_id)
}

/// Approximate character level when each act is first entered, derived from
/// vendored area data: act 1 starts at level 1, and act N is entered at
/// roughly the level of act N-1's town (the last area of the previous act).
pub fn act_entry_levels(areas: &AreaMap) -> BTreeMap<u8, u16> {
    let mut levels: BTreeMap<u8, u16> = BTreeMap::new();
    levels.insert(1, 1);
    for area in areas.values() {
        if !area.is_town_area {
            continue;
        }
        let Some(level) = area.level else { continue };
        levels.entry(area.act + 1).or_insert(u16::from(level));
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::load_vendored;

    fn vendored() -> (AreaMap, QuestMap) {
        load_vendored().expect("vendored data loads")
    }

    #[test]
    fn witch_gets_freezing_pulse_as_an_act_1_quest_reward() {
        let (_, quests) = vendored();
        let avail = availability_for_class(&quests, "Witch");
        let fp = avail
            .get("Metadata/Items/Gems/SkillGemFreezingPulse")
            .expect("freezing pulse available");
        assert_eq!(fp.act, 1);
        assert_eq!(fp.quest_id, "a1q1");
        assert_eq!(fp.source, AvailabilitySource::QuestReward);
    }

    #[test]
    fn marauder_cannot_get_freezing_pulse_before_siosa_in_act_3() {
        let (_, quests) = vendored();
        let avail = availability_for_class(&quests, "Marauder");
        let fp = avail
            .get("Metadata/Items/Gems/SkillGemFreezingPulse")
            .expect("freezing pulse available via Siosa");
        assert_eq!(fp.act, 3);
        assert_eq!(fp.quest_id, "a3q12");
        assert_eq!(fp.source, AvailabilitySource::Vendor);
        assert_eq!(fp.npc.as_deref(), Some("Siosa"));
    }

    #[test]
    fn ranger_gets_toxic_rain_as_an_act_1_quest_reward() {
        let (_, quests) = vendored();
        let avail = availability_for_class(&quests, "Ranger");
        let tr = avail
            .get("Metadata/Items/Gems/SkillGemToxicRain")
            .expect("toxic rain available");
        assert_eq!(tr.act, 1);
        assert_eq!(tr.source, AvailabilitySource::QuestReward);
    }

    #[test]
    fn duelist_buys_toxic_rain_from_nessa_in_act_1() {
        let (_, quests) = vendored();
        let avail = availability_for_class(&quests, "Duelist");
        let tr = avail
            .get("Metadata/Items/Gems/SkillGemToxicRain")
            .expect("toxic rain available");
        assert_eq!(tr.act, 1);
        assert_eq!(tr.source, AvailabilitySource::Vendor);
        assert_eq!(tr.npc.as_deref(), Some("Nessa"));
    }

    #[test]
    fn unrestricted_offers_apply_to_unknown_classes() {
        let (_, quests) = vendored();
        let avail = availability_for_class(&quests, "NotARealClass");
        let fb = avail
            .get("Metadata/Items/Gems/SkillGemFrostblink")
            .expect("frostblink available via unrestricted vendor offer");
        assert_eq!(fb.act, 1);
        assert_eq!(fb.source, AvailabilitySource::Vendor);
        assert_eq!(fb.npc.as_deref(), Some("Nessa"));
    }

    #[test]
    fn quest_reward_beats_vendor_offer_within_the_same_act() {
        let (_, quests) = vendored();
        // Frostblink in act 1 is both a quest reward (Witch among others) and
        // an unrestricted Nessa vendor offer from the same quest.
        let avail = availability_for_class(&quests, "Witch");
        let fb = avail
            .get("Metadata/Items/Gems/SkillGemFrostblink")
            .expect("frostblink available");
        assert_eq!(fb.act, 1);
        assert_eq!(fb.source, AvailabilitySource::QuestReward);
    }

    #[test]
    fn act_entry_levels_track_previous_act_town_levels() {
        let (areas, _) = vendored();
        let entry = act_entry_levels(&areas);
        assert_eq!(entry.get(&1), Some(&1));
        assert_eq!(entry.get(&2), Some(&13));
        assert_eq!(entry.get(&3), Some(&23));
        assert_eq!(entry.get(&4), Some(&33));
        assert_eq!(entry.get(&10), Some(&67));
    }
}
