//! Route progress engine: tracks a player's position through a compiled
//! campaign route as `on_area_entered` events arrive, per the advancement
//! policy documented in the plan.

use content::compile::ContentPack;
use content::game_data::AreaMap;
use content::walk::CompiledStep;
use serde::Serialize;

/// How many groups ahead of the active group still count as "on route" for
/// tier-2 advancement (see `RouteEngine::on_area_entered`).
pub const LOOKAHEAD_GROUPS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum StepStatus {
    Pending,
    Done,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AdvanceKind {
    InPlace,
    Advanced,
    Resumed,
    OffRoute { area_id: String },
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Advance {
    pub kind: AdvanceKind,
    pub newly_done: Vec<usize>,
    pub newly_skipped: Vec<usize>,
}

pub struct RouteEngine {
    steps: Vec<CompiledStep>,
    statuses: Vec<StepStatus>,
    areas: AreaMap,
    cursor: usize,
    off_route: Option<String>,
}

impl RouteEngine {
    pub fn new(steps: Vec<CompiledStep>, areas: AreaMap) -> Self {
        let statuses = vec![StepStatus::Pending; steps.len()];
        Self {
            steps,
            statuses,
            areas,
            cursor: 0,
            off_route: None,
        }
    }

    pub fn from_pack(pack: &ContentPack, areas: AreaMap) -> Self {
        let steps = pack
            .acts
            .iter()
            .flat_map(|a| a.steps.iter().cloned())
            .collect();
        Self::new(steps, areas)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn steps(&self) -> &[CompiledStep] {
        &self.steps
    }

    pub fn statuses(&self) -> &[StepStatus] {
        &self.statuses
    }

    pub fn is_complete(&self) -> bool {
        self.cursor >= self.steps.len()
    }

    /// Index one-past the end of the contiguous run of steps sharing the
    /// `area_context` of `self.steps[start]` (a "group").
    fn group_end(&self, start: usize) -> usize {
        let ctx = &self.steps[start].area_context;
        let mut i = start + 1;
        while i < self.steps.len() && &self.steps[i].area_context == ctx {
            i += 1;
        }
        i
    }

    pub fn active_steps(&self) -> &[CompiledStep] {
        if self.is_complete() {
            return &[];
        }
        &self.steps[self.cursor..self.group_end(self.cursor)]
    }

    pub fn active_area(&self) -> Option<&str> {
        self.steps.get(self.cursor).map(|s| s.area_context.as_str())
    }

    pub fn off_route(&self) -> Option<&str> {
        self.off_route.as_deref()
    }

    pub fn act(&self) -> Option<u8> {
        self.steps.get(self.cursor).map(|s| s.act)
    }

    pub fn next_transition_area(&self) -> Option<&str> {
        if self.is_complete() {
            return None;
        }
        let next = self.group_end(self.cursor);
        self.steps.get(next).map(|s| s.area_context.as_str())
    }

    /// Start indices of every group from the active group onward (index 0
    /// is always the active group itself).
    fn forward_group_starts(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut i = self.cursor;
        while i < self.steps.len() {
            starts.push(i);
            i = self.group_end(i);
        }
        starts
    }

    pub fn on_area_entered(&mut self, area_id: &str) -> Advance {
        let no_op = |kind| Advance {
            kind,
            newly_done: vec![],
            newly_skipped: vec![],
        };

        if self.is_complete() {
            return no_op(AdvanceKind::Ignored);
        }
        if self.active_area() == Some(area_id) {
            let kind = if self.off_route.take().is_some() {
                AdvanceKind::Resumed
            } else {
                AdvanceKind::InPlace
            };
            return no_op(kind);
        }

        let starts = self.forward_group_starts();
        let matched = starts
            .iter()
            .enumerate()
            .skip(1) // index 0 is the active group, already handled above
            .find(|&(_, &s)| self.steps[s].area_context == area_id);

        let is_town = self
            .areas
            .get(area_id)
            .map(|a| a.is_town_area)
            .unwrap_or(false);

        match matched {
            // Tier 2: within lookahead, regardless of town-ness.
            // Tier 3: any forward group, but only for non-town areas.
            Some((group_offset, &target)) if group_offset <= LOOKAHEAD_GROUPS || !is_town => {
                let mut newly_done = Vec::new();
                let mut newly_skipped = Vec::new();
                let active_end = self.group_end(self.cursor);
                for i in self.cursor..target {
                    if self.statuses[i] == StepStatus::Pending {
                        if i < active_end {
                            self.statuses[i] = StepStatus::Done;
                            newly_done.push(i);
                        } else {
                            self.statuses[i] = StepStatus::Skipped;
                            newly_skipped.push(i);
                        }
                    }
                }
                self.cursor = target;
                self.off_route = None;

                // Design decision (see plan4-task-1-brief.md "Note on
                // completion"): entering the FINAL group of the route
                // completes the route immediately rather than leaving the
                // engine parked "on" the last group awaiting a further
                // event that would never come (there is nothing beyond the
                // campaign's last step to trigger a subsequent advance).
                // So: if the group we just entered is the last group,
                // mark its steps Done and set cursor = steps.len() now.
                if self.group_end(target) == self.steps.len() {
                    let last_end = self.steps.len();
                    for i in target..last_end {
                        if self.statuses[i] == StepStatus::Pending {
                            self.statuses[i] = StepStatus::Done;
                            newly_done.push(i);
                        }
                    }
                    self.cursor = last_end;
                }

                Advance {
                    kind: AdvanceKind::Advanced,
                    newly_done,
                    newly_skipped,
                }
            }
            _ => {
                if self.off_route.as_deref() == Some(area_id) {
                    return no_op(AdvanceKind::Ignored);
                }
                self.off_route = Some(area_id.to_string());
                no_op(AdvanceKind::OffRoute {
                    area_id: area_id.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use content::compile::{Variant, compile_route_pack};
    use content::game_data::load_vendored;

    fn engine() -> RouteEngine {
        let (areas, _) = load_vendored().unwrap();
        let pack = compile_route_pack(Variant::LeagueStart).unwrap();
        RouteEngine::from_pack(&pack, areas)
    }

    #[test]
    fn starts_at_first_group_twilight_strand() {
        let e = engine();
        assert_eq!(e.active_area(), Some("1_1_1"));
        assert_eq!(e.act(), Some(1));
        assert!(!e.active_steps().is_empty());
        assert!(!e.is_complete());
    }

    #[test]
    fn advances_through_expected_lookahead() {
        let mut e = engine();
        let a = e.on_area_entered("1_1_town");
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_town"));
        // The bypassed 1_1_1 group is Done (it was the active group).
        assert!(
            a.newly_done
                .iter()
                .all(|&i| e.steps()[i].area_context == "1_1_1")
        );
        assert!(a.newly_skipped.is_empty());

        let a = e.on_area_entered("1_1_2");
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_2"));
    }

    #[test]
    fn duplicate_event_is_idempotent_in_place() {
        let mut e = engine();
        e.on_area_entered("1_1_town");
        let a = e.on_area_entered("1_1_town");
        assert_eq!(a.kind, AdvanceKind::InPlace);
        assert!(a.newly_done.is_empty() && a.newly_skipped.is_empty());
    }

    #[test]
    fn unexpected_town_visit_is_off_route_and_resumable() {
        let mut e = engine();
        e.on_area_entered("1_1_town");
        e.on_area_entered("1_1_2"); // active: The Coast group
        let cursor_before = e.cursor();

        // Early logout: town is not within lookahead here and towns never
        // trigger the distant-forward jump.
        let a = e.on_area_entered("1_1_town");
        assert_eq!(
            a.kind,
            AdvanceKind::OffRoute {
                area_id: "1_1_town".into()
            }
        );
        assert_eq!(e.cursor(), cursor_before);
        assert_eq!(e.off_route(), Some("1_1_town"));
        assert_eq!(e.active_area(), Some("1_1_2")); // progress untouched

        // Coming back to the active area resumes.
        let a = e.on_area_entered("1_1_2");
        assert_eq!(a.kind, AdvanceKind::Resumed);
        assert_eq!(e.off_route(), None);
    }

    #[test]
    fn distant_forward_field_zone_jumps_with_skips() {
        let mut e = engine();
        e.on_area_entered("1_1_town");
        e.on_area_entered("1_1_2");
        // Jump far ahead to The Ledge (1_1_5), skipping several groups —
        // beyond lookahead, but a non-town field zone.
        let a = e.on_area_entered("1_1_5");
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_5"));
        assert!(
            !a.newly_skipped.is_empty(),
            "bypassed groups must be Skipped"
        );
        // The formerly active Coast group counts as Done, not Skipped.
        assert!(
            a.newly_done
                .iter()
                .any(|&i| e.steps()[i].area_context == "1_1_2")
        );
    }

    #[test]
    fn unknown_area_id_is_off_route_not_panic() {
        let mut e = engine();
        let a = e.on_area_entered("totally_bogus");
        assert_eq!(
            a.kind,
            AdvanceKind::OffRoute {
                area_id: "totally_bogus".into()
            }
        );
    }

    #[test]
    fn cursor_is_monotonic_across_full_route_replay() {
        // Feed every group's area_context in order; cursor must never move
        // backward and must complete the route.
        let mut e = engine();
        let contexts: Vec<String> = {
            let mut cs = Vec::new();
            for s in e.steps() {
                if cs.last() != Some(&s.area_context) {
                    cs.push(s.area_context.clone());
                }
            }
            cs
        };
        let mut last_cursor = 0;
        for c in &contexts {
            e.on_area_entered(c);
            assert!(e.cursor() >= last_cursor, "cursor moved backward at {c}");
            last_cursor = e.cursor();
        }
        assert!(e.is_complete());
        assert_eq!(e.act(), None);
        assert!(e.active_steps().is_empty());
        // After completion further events are Ignored.
        assert_eq!(e.on_area_entered("1_1_1").kind, AdvanceKind::Ignored);
    }

    #[test]
    fn next_transition_area_names_the_following_group() {
        let mut e = engine();
        assert_eq!(e.next_transition_area(), Some("1_1_town"));
        e.on_area_entered("1_1_town");
        assert_eq!(e.next_transition_area(), Some("1_1_2"));
    }
}
