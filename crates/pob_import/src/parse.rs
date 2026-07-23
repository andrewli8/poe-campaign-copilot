//! quick-xml pull parsing of PoB build XML into a `LevelingBuildPlan`.
//!
//! Total: elements/attributes we don't recognize are ignored, and a gem
//! name with no vendored match just leaves its `required_level`/`is_support`
//! as `None` — nothing here errors on unexpected shape, only on
//! genuinely malformed XML (`PobError::Xml`).

use std::collections::BTreeSet;

use content::game_data::{GemMap, gems_by_name};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::{GemPlan, LevelingBuildPlan, Milestone, PobError, Reliability, SkillSetPlan};

pub fn parse_build(xml: &str, gems: &GemMap) -> Result<LevelingBuildPlan, PobError> {
    let by_name = gems_by_name(gems);
    let mut state = ParseState::default();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) => state.handle_open(&e, false, &by_name)?,
            Event::Empty(e) => state.handle_open(&e, true, &by_name)?,
            Event::End(e) => state.handle_close(e.name().as_ref()),
            Event::Text(e) => {
                if state.in_notes {
                    state.notes_buf.push_str(&e.unescape().map_err(xml_err)?);
                }
            }
            Event::CData(e) => {
                if state.in_notes {
                    state.notes_buf.push_str(&e.decode().map_err(xml_err)?);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(state.into_plan())
}

fn xml_err(e: impl std::fmt::Display) -> PobError {
    PobError::Xml(e.to_string())
}

#[derive(Default)]
struct ParseState {
    class_name: String,
    ascend_name: Option<String>,
    skill_sets: Vec<SkillSetPlan>,
    passive_spec_titles: Vec<String>,
    in_skills: bool,
    current_set: Option<SkillSetPlan>,
    in_notes: bool,
    notes_buf: String,
    notes: Option<String>,
}

impl ParseState {
    fn handle_open(
        &mut self,
        e: &BytesStart,
        is_empty: bool,
        by_name: &std::collections::BTreeMap<String, content::game_data::Gem>,
    ) -> Result<(), PobError> {
        match e.name().as_ref() {
            b"Build" => self.open_build(e)?,
            b"Skills" => self.in_skills = true,
            b"SkillSet" => self.open_skill_set(e, is_empty)?,
            b"Skill" => self.open_skill(),
            b"Gem" => self.open_gem(e, by_name)?,
            b"Spec" => {
                if let Some(title) = get_attr(e, "title")? {
                    self.passive_spec_titles.push(title);
                }
            }
            b"Notes" => {
                if is_empty {
                    self.notes = None;
                } else {
                    self.in_notes = true;
                    self.notes_buf.clear();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_close(&mut self, name: &[u8]) {
        match name {
            b"Skills" => {
                self.in_skills = false;
                self.close_skill_set();
            }
            b"SkillSet" => self.close_skill_set(),
            b"Notes" => {
                self.in_notes = false;
                let trimmed = self.notes_buf.trim();
                self.notes = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            _ => {}
        }
    }

    fn open_build(&mut self, e: &BytesStart) -> Result<(), PobError> {
        self.class_name = get_attr(e, "className")?.unwrap_or_default();
        self.ascend_name =
            get_attr(e, "ascendClassName")?.filter(|ascend| !ascend.is_empty() && ascend != "None");
        Ok(())
    }

    fn open_skill_set(&mut self, e: &BytesStart, is_empty: bool) -> Result<(), PobError> {
        self.close_skill_set();
        let title = get_attr(e, "title")?.unwrap_or_default();
        let level_range = parse_level_range(&title);
        let set = SkillSetPlan {
            title,
            level_range,
            gems: Vec::new(),
        };
        if is_empty {
            self.skill_sets.push(set);
        } else {
            self.current_set = Some(set);
        }
        Ok(())
    }

    fn open_skill(&mut self) {
        if self.in_skills && self.current_set.is_none() {
            self.current_set = Some(SkillSetPlan {
                title: "Default".to_string(),
                level_range: None,
                gems: Vec::new(),
            });
        }
    }

    fn open_gem(
        &mut self,
        e: &BytesStart,
        by_name: &std::collections::BTreeMap<String, content::game_data::Gem>,
    ) -> Result<(), PobError> {
        let Some(set) = self.current_set.as_mut() else {
            return Ok(());
        };
        let name = get_attr(e, "nameSpec")?.unwrap_or_default();
        let enabled = get_attr(e, "enabled")?
            .map(|v| v == "true")
            .unwrap_or(false);
        // Real Path of Building exports write support gems' `nameSpec`
        // WITHOUT the " Support" suffix that the vendored gem data's `name`
        // field carries (e.g. nameSpec="Pierce", not "Pierce Support"), so
        // an exact-name lookup misses every support gem in a real export.
        // Fall back to the suffixed name before giving up.
        let enriched = by_name
            .get(&name)
            .or_else(|| by_name.get(&format!("{name} Support")));
        set.gems.push(GemPlan {
            name,
            required_level: enriched.map(|g| g.required_level),
            is_support: enriched.map(|g| g.is_support),
            enabled,
        });
        Ok(())
    }

    fn close_skill_set(&mut self) {
        if let Some(set) = self.current_set.take() {
            self.skill_sets.push(set);
        }
    }

    fn into_plan(self) -> LevelingBuildPlan {
        let milestones = build_milestones(&self.skill_sets);
        let reliability = if milestones.is_empty() {
            Reliability::Unsupported
        } else {
            Reliability::Structured
        };
        LevelingBuildPlan {
            class_name: self.class_name,
            ascend_name: self.ascend_name,
            skill_sets: self.skill_sets,
            passive_spec_titles: self.passive_spec_titles,
            notes: self.notes,
            milestones,
            reliability,
        }
    }
}

fn get_attr(e: &BytesStart, name: &str) -> Result<Option<String>, PobError> {
    match e.try_get_attribute(name).map_err(xml_err)? {
        Some(attr) => Ok(Some(attr.unescape_value().map_err(xml_err)?.into_owned())),
        None => Ok(None),
    }
}

/// PoE's level cap. A skill-set title is free-text set by whoever authored
/// the PoB build/pastebin, so a crafted title (e.g. "65531-65535") could
/// otherwise smuggle a near-`u16::MAX` value into a milestone's `level` —
/// nonsense for gameplay purposes and a source of `u16` overflow downstream
/// (see `composer::build_reminders_for`). Clamping at the source means every
/// consumer of `SkillSetPlan::level_range` / `Milestone::level` gets a
/// value that's always a plausible player level.
const MAX_PLAYER_LEVEL: u16 = 100;

/// First `A-B`/`A–B` numeric pair anywhere in the title, else a leading
/// bare number `A` (→ `(A, A)`), else `None`. Both endpoints are clamped to
/// `MAX_PLAYER_LEVEL`.
fn parse_level_range(title: &str) -> Option<(u16, u16)> {
    find_dash_pair(title)
        .or_else(|| leading_digits(title).map(|a| (a, a)))
        .map(|(a, b)| (a.min(MAX_PLAYER_LEVEL), b.min(MAX_PLAYER_LEVEL)))
}

fn find_dash_pair(title: &str) -> Option<(u16, u16)> {
    for (idx, ch) in title.char_indices() {
        if ch == '-' || ch == '\u{2013}' {
            let before = title[..idx].trim_end();
            let after = title[idx + ch.len_utf8()..].trim_start();
            if let (Some(a), Some(b)) = (trailing_digits(before), leading_digits(after)) {
                return Some((a, b));
            }
        }
    }
    None
}

fn leading_digits(s: &str) -> Option<u16> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn trailing_digits(s: &str) -> Option<u16> {
    let digits: String = s
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// (a) each set with a parsed range start >= 2 gets a "switch" milestone;
/// (b) each unique enabled non-support gem with a known required_level >= 2
/// gets a "gem available" milestone. Deduped by name, sorted by
/// (level, label).
fn build_milestones(skill_sets: &[SkillSetPlan]) -> Vec<Milestone> {
    let mut milestones: Vec<Milestone> = skill_sets
        .iter()
        .filter_map(|set| {
            let (start, _) = set.level_range?;
            (start >= 2).then(|| Milestone {
                level: start,
                label: format!("Switch to skill set '{}'", set.title),
                reliability: Reliability::Structured,
            })
        })
        .collect();

    let mut seen_gems = BTreeSet::new();
    for gem in skill_sets.iter().flat_map(|set| &set.gems) {
        if !gem.enabled || gem.is_support == Some(true) {
            continue;
        }
        let Some(level) = gem.required_level else {
            continue;
        };
        if level < 2 || !seen_gems.insert(gem.name.clone()) {
            continue;
        }
        milestones.push(Milestone {
            level: u16::from(level),
            label: format!("Gem available: {}", gem.name),
            reliability: Reliability::Structured,
        });
    }

    milestones.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.label.cmp(&b.label)));
    milestones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_level_range_clamps_a_crafted_huge_range_to_the_level_cap() {
        assert_eq!(parse_level_range("65531-65535"), Some((100, 100)));
    }

    #[test]
    fn parse_level_range_clamps_only_the_endpoint_that_exceeds_the_cap() {
        assert_eq!(parse_level_range("40-65535"), Some((40, 100)));
    }

    #[test]
    fn parse_level_range_leaves_in_range_values_untouched() {
        assert_eq!(parse_level_range("1-12"), Some((1, 12)));
        assert_eq!(parse_level_range("13-32"), Some((13, 32)));
    }

    #[test]
    fn parse_level_range_clamps_a_bare_leading_number() {
        assert_eq!(parse_level_range("65535"), Some((100, 100)));
    }
}
