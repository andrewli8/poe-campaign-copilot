//! Session tracking: pairs raw log events into authoritative area
//! transitions using the vendored area map.

use content::game_data::AreaMap;
use event_parser::RawEvent;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    SessionStarted {
        at: String,
    },
    AreaEntered {
        area_id: String,
        display_name: String,
        act: u8,
        area_level: u8,
        is_town: bool,
        new_instance: bool,
        at: String,
    },
    LevelUp {
        character: String,
        class: String,
        level: u16,
        at: String,
    },
    Slain {
        character: String,
        at: String,
    },
    UnresolvedArea {
        display_name: String,
        at: String,
    },
}

struct PendingGenerated {
    area_id: String,
    area_level: u8,
    seed: u64,
}

pub struct SessionTracker {
    areas: AreaMap,
    started: bool,
    current_area_id: Option<String>,
    current_act: Option<u8>,
    pending: Option<PendingGenerated>,
    last_seed_by_area: std::collections::BTreeMap<String, u64>,
}

impl SessionTracker {
    pub fn new(areas: AreaMap) -> Self {
        Self {
            areas,
            started: false,
            current_area_id: None,
            current_act: None,
            pending: None,
            last_seed_by_area: std::collections::BTreeMap::new(),
        }
    }

    pub fn current_area_id(&self) -> Option<&str> {
        self.current_area_id.as_deref()
    }

    pub fn current_act(&self) -> Option<u8> {
        self.current_act
    }

    pub fn on_raw(&mut self, event: &RawEvent) -> Vec<SessionEvent> {
        let mut out = Vec::new();
        if !self.started
            && let Some(at) = event_at(event)
        {
            self.started = true;
            out.push(SessionEvent::SessionStarted { at: at.to_string() });
        }
        match event {
            RawEvent::AreaGenerated {
                area_id,
                area_level,
                seed,
                ..
            } => {
                self.pending = Some(PendingGenerated {
                    area_id: area_id.clone(),
                    area_level: *area_level,
                    seed: *seed,
                });
            }
            RawEvent::AreaEnteredName { display_name, at } => {
                out.extend(self.resolve_entry(display_name, at));
            }
            RawEvent::LevelUp {
                character,
                class,
                level,
                at,
            } => {
                out.push(SessionEvent::LevelUp {
                    character: character.clone(),
                    class: class.clone(),
                    level: *level,
                    at: at.clone(),
                });
            }
            RawEvent::Slain { character, at } => {
                out.push(SessionEvent::Slain {
                    character: character.clone(),
                    at: at.clone(),
                });
            }
            RawEvent::Unknown { .. } => {}
        }
        out
    }

    fn resolve_entry(&mut self, display_name: &str, at: &str) -> Vec<SessionEvent> {
        // Authoritative path: the pending Generating line names this area.
        let resolved: Option<(String, u8, u64)> = match self.pending.take() {
            Some(p)
                if self
                    .areas
                    .get(&p.area_id)
                    .is_some_and(|a| a.name == display_name) =>
            {
                Some((p.area_id, p.area_level, p.seed))
            }
            other => {
                self.pending = other; // keep unrelated pending for later
                self.resolve_by_name(display_name)
            }
        };

        let Some((area_id, area_level, seed)) = resolved else {
            return vec![SessionEvent::UnresolvedArea {
                display_name: display_name.to_string(),
                at: at.to_string(),
            }];
        };

        let area = &self.areas[&area_id];
        let new_instance = self.last_seed_by_area.get(&area_id) != Some(&seed);
        self.last_seed_by_area.insert(area_id.clone(), seed);
        self.current_area_id = Some(area_id.clone());
        self.current_act = Some(area.act);

        vec![SessionEvent::AreaEntered {
            area_id,
            display_name: display_name.to_string(),
            act: area.act,
            area_level,
            is_town: area.is_town_area,
            new_instance,
            at: at.to_string(),
        }]
    }

    /// Fallback when no Generating line was captured: resolve by display
    /// name, preferring the candidate whose act is closest to the current
    /// act (ties break to the lower act). `content::game_data::Area` does
    /// not carry a per-area game level, so `area_level` defaults to 0 in
    /// this path. Seed 0 marks "unknown instance" (treated as new only if
    /// no prior seed is recorded).
    fn resolve_by_name(&self, display_name: &str) -> Option<(String, u8, u64)> {
        let current = self.current_act.unwrap_or(1) as i16;
        self.areas
            .values()
            .filter(|a| a.name == display_name)
            .min_by_key(|a| ((a.act as i16 - current).abs(), a.act))
            .map(|a| {
                let seed = self.last_seed_by_area.get(&a.id).copied().unwrap_or(0);
                (a.id.clone(), 0u8, seed)
            })
    }
}

fn event_at(event: &RawEvent) -> Option<&str> {
    match event {
        RawEvent::AreaGenerated { at, .. }
        | RawEvent::AreaEnteredName { at, .. }
        | RawEvent::LevelUp { at, .. }
        | RawEvent::Slain { at, .. }
        | RawEvent::Unknown { at, .. } => Some(at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::game_data::load_vendored;

    fn track(events: &[RawEvent]) -> (SessionTracker, Vec<SessionEvent>) {
        let (areas, _) = load_vendored().unwrap();
        let mut t = SessionTracker::new(areas);
        let mut out = Vec::new();
        for e in events {
            out.extend(t.on_raw(e));
        }
        (t, out)
    }

    fn gen_event(area_id: &str, level: u8, seed: u64) -> RawEvent {
        RawEvent::AreaGenerated {
            area_id: area_id.into(),
            area_level: level,
            seed,
            at: "t".into(),
        }
    }

    fn entered(name: &str) -> RawEvent {
        RawEvent::AreaEnteredName {
            display_name: name.into(),
            at: "t".into(),
        }
    }

    #[test]
    fn pairs_generated_and_entered_into_area_entered() {
        let (t, out) = track(&[gen_event("1_1_1", 1, 42), entered("The Twilight Strand")]);
        assert_eq!(out[0], SessionEvent::SessionStarted { at: "t".into() });
        assert_eq!(
            out[1],
            SessionEvent::AreaEntered {
                area_id: "1_1_1".into(),
                display_name: "The Twilight Strand".into(),
                act: 1,
                area_level: 1,
                is_town: false,
                new_instance: true,
                at: "t".into(),
            }
        );
        assert_eq!(t.current_area_id(), Some("1_1_1"));
        assert_eq!(t.current_act(), Some(1));
    }

    #[test]
    fn town_flag_and_same_seed_revisit() {
        let (_, out) = track(&[
            gen_event("1_1_town", 1, 7),
            entered("Lioneye's Watch"),
            gen_event("1_1_2", 2, 500),
            entered("The Coast"),
            gen_event("1_1_town", 1, 7), // waypoint back, same instance
            entered("Lioneye's Watch"),
        ]);
        let entries: Vec<_> = out
            .iter()
            .filter_map(|e| match e {
                SessionEvent::AreaEntered {
                    area_id,
                    is_town,
                    new_instance,
                    ..
                } => Some((area_id.as_str(), *is_town, *new_instance)),
                _ => None,
            })
            .collect();
        assert_eq!(
            entries,
            vec![
                ("1_1_town", true, true),
                ("1_1_2", false, true),
                ("1_1_town", true, false),
            ]
        );
    }

    #[test]
    fn new_seed_means_new_instance() {
        let (_, out) = track(&[
            gen_event("1_1_2", 2, 500),
            entered("The Coast"),
            gen_event("1_1_2", 2, 501), // re-rolled instance
            entered("The Coast"),
        ]);
        let flags: Vec<bool> = out
            .iter()
            .filter_map(|e| match e {
                SessionEvent::AreaEntered { new_instance, .. } => Some(*new_instance),
                _ => None,
            })
            .collect();
        assert_eq!(flags, vec![true, true]);
    }

    #[test]
    fn name_only_fallback_prefers_current_act() {
        // "The Coast" exists in act 1 (1_1_2) and act 6 (2_6_2).
        // After entering an act 6 area, a name-only entry resolves to act 6.
        let (_, out) = track(&[
            gen_event("2_6_town", 40, 1),
            entered("Lioneye's Watch"),
            entered("The Coast"), // no Generating line captured
        ]);
        let last = out.last().unwrap();
        match last {
            SessionEvent::AreaEntered { area_id, act, .. } => {
                assert_eq!(area_id, "2_6_2");
                assert_eq!(*act, 6);
            }
            other => panic!("expected AreaEntered, got {other:?}"),
        }
    }

    #[test]
    fn unknown_display_name_yields_unresolved() {
        let (_, out) = track(&[entered("Some Future League Zone")]);
        assert!(matches!(
            out.last().unwrap(),
            SessionEvent::UnresolvedArea { .. }
        ));
    }

    #[test]
    fn levelup_and_slain_pass_through() {
        let (_, out) = track(&[
            RawEvent::LevelUp {
                character: "W".into(),
                class: "Ranger".into(),
                level: 5,
                at: "t".into(),
            },
            RawEvent::Slain {
                character: "W".into(),
                at: "t".into(),
            },
        ]);
        assert!(matches!(out[1], SessionEvent::LevelUp { .. }));
        assert!(matches!(out[2], SessionEvent::Slain { .. }));
    }

    #[test]
    fn unknown_raw_events_produce_nothing() {
        let (_, out) = track(&[RawEvent::Unknown {
            fingerprint: "x".into(),
            at: "t".into(),
        }]);
        // Only SessionStarted from first event
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], SessionEvent::SessionStarted { .. }));
    }
}
