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
    /// Area contexts of every step that has fallen behind the cursor, i.e.
    /// zones the route has already visited at least once. Consulted by the
    /// tier-3 distant-forward-jump guard below: many non-town field zones
    /// are "revisit magnets" that recur at multiple points in a compiled
    /// route (e.g. Aspirants' Plaza appears both before and after the
    /// normal Labyrinth). Without this guard, walking back into such a
    /// zone after the route has already passed it would match the LATER
    /// occurrence far down the route and silently teleport the cursor
    /// there, since tier 3 otherwise accepts any forward non-town match.
    ///
    /// This guard only applies to SAME-INSTANCE re-entries (portal back
    /// into a zone you already generated). A genuinely NEW instance of a
    /// forward context (`new_instance == true` on `on_area_entered`, see
    /// `session::SessionEvent::AreaEntered`) bypasses this guard entirely:
    /// a fresh instance can only be reached by actually playing forward, so
    /// it's real progress even if the context happens to repeat or is a
    /// town.
    visited_contexts: std::collections::HashSet<String>,
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
            visited_contexts: std::collections::HashSet::new(),
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

    /// Whether `area_id` is a known town area (unknown ids are treated as
    /// non-town).
    fn is_town(&self, area_id: &str) -> bool {
        self.areas
            .get(area_id)
            .map(|a| a.is_town_area)
            .unwrap_or(false)
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

    pub fn on_area_entered(&mut self, area_id: &str, new_instance: bool) -> Advance {
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

        let is_town = self.is_town(area_id);
        // Tier 3 only ever jumps to a zone's FIRST occurrence in the route
        // for a SAME-INSTANCE re-entry; see `visited_contexts` doc comment
        // for the revisit-magnet rationale this guards against. A
        // genuinely NEW instance of a forward zone is real progression
        // (the player could only get there by actually playing through
        // it), so it bypasses the guard entirely — "further along the
        // route" is taken as proof the earlier steps are already done,
        // even if that context is a town or repeats later in the route.
        let already_visited = self.visited_contexts.contains(area_id);

        match matched {
            // Tier 2: within lookahead, regardless of town-ness or prior
            // visits (a short back-and-forth is normal play).
            // Tier 3: any forward group, when either (a) it's a non-town
            // area the route has never visited before [existing guard], or
            // (b) the client reports a brand-new zone instance, which is
            // unambiguous forward progress regardless of town-ness or
            // revisit history [new: "follow the player forward"].
            Some((group_offset, &target))
                if group_offset <= LOOKAHEAD_GROUPS
                    || (!is_town && !already_visited)
                    || new_instance =>
            {
                let mut newly_done = Vec::new();
                let mut newly_skipped = Vec::new();
                let active_end = self.group_end(self.cursor);
                for i in self.cursor..target {
                    self.visited_contexts
                        .insert(self.steps[i].area_context.clone());
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
                        self.visited_contexts
                            .insert(self.steps[i].area_context.clone());
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
        let a = e.on_area_entered("1_1_town", true);
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_town"));
        // The bypassed 1_1_1 group is Done (it was the active group).
        assert!(
            a.newly_done
                .iter()
                .all(|&i| e.steps()[i].area_context == "1_1_1")
        );
        assert!(a.newly_skipped.is_empty());

        let a = e.on_area_entered("1_1_2", true);
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_2"));
    }

    #[test]
    fn duplicate_event_is_idempotent_in_place() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        let a = e.on_area_entered("1_1_town", false);
        assert_eq!(a.kind, AdvanceKind::InPlace);
        assert!(a.newly_done.is_empty() && a.newly_skipped.is_empty());
    }

    #[test]
    fn unexpected_town_visit_is_off_route_and_resumable() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true); // active: The Coast group
        let cursor_before = e.cursor();

        // Early logout: town is not within lookahead here, and a portal
        // back into the SAME town instance (new_instance = false) never
        // triggers the distant-forward jump.
        let a = e.on_area_entered("1_1_town", false);
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
        let a = e.on_area_entered("1_1_2", false);
        assert_eq!(a.kind, AdvanceKind::Resumed);
        assert_eq!(e.off_route(), None);
    }

    #[test]
    fn distant_forward_field_zone_jumps_with_skips() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true);
        // Jump far ahead to The Ledge (1_1_5), skipping several groups —
        // beyond lookahead, but a non-town field zone.
        let a = e.on_area_entered("1_1_5", true);
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

    /// Every group's start index and its `area_context`, one entry per
    /// group in route order (mirrors `forward_group_starts` from cursor 0).
    fn all_group_starts_and_contexts(e: &RouteEngine) -> Vec<(usize, String)> {
        let mut v = Vec::new();
        let mut i = 0;
        while i < e.steps().len() {
            v.push((i, e.steps()[i].area_context.clone()));
            i = e.group_end(i);
        }
        v
    }

    #[test]
    fn fresh_distant_field_zone_still_jumps() {
        // A non-town field zone that appears ONLY ahead (never visited
        // before) must still trigger the tier-3 distant jump.
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true);
        let a = e.on_area_entered("1_1_5", true);
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_5"));
    }

    #[test]
    fn revisited_field_zone_beyond_lookahead_is_off_route_not_jump() {
        // Data-driven: find a non-town context that occurs at two group
        // indices (gi, gj) far enough apart that, once the route has
        // passed group `gi` and moved on to group `gi + 1`, re-entering
        // that context would offer a tier-3 (beyond-lookahead) match to
        // the LATER occurrence at `gj`. The reviewer confirmed 33 such
        // non-town revisit contexts exist in the real league-start route.
        let e = engine();
        let groups = all_group_starts_and_contexts(&e);

        let mut found: Option<(usize, usize, String)> = None;
        'outer: for gi in 0..groups.len() {
            if e.is_town(&groups[gi].1) {
                continue;
            }
            // gj must be far enough past (gi + 1) that the group offset
            // from the new active group exceeds LOOKAHEAD_GROUPS.
            let min_gj = gi + LOOKAHEAD_GROUPS + 2;
            for gj in min_gj..groups.len() {
                if groups[gi].1 == groups[gj].1 {
                    found = Some((gi, gj, groups[gi].1.clone()));
                    break 'outer;
                }
            }
        }
        let (gi, gj, ctx) = found.expect(
            "expected at least one non-town context to repeat far enough apart in the route \
             (reviewer found 33); route data may have changed",
        );

        // Advance sequentially through every group up to and including
        // `gi`, so group `gi`'s context falls behind the cursor (visited)
        // and group `gi + 1` becomes active.
        let mut e = engine();
        for (_, ctx) in &groups[0..=gi + 1] {
            e.on_area_entered(ctx, true);
        }
        assert_eq!(e.cursor(), groups[gi + 1].0, "test setup: cursor mismatch");

        let group_offset = gj - (gi + 1);
        assert!(
            group_offset > LOOKAHEAD_GROUPS,
            "test setup: target must be beyond lookahead, got offset {group_offset}"
        );

        let cursor_before = e.cursor();
        // Portal back to the SAME instance of this zone (not a freshly
        // generated one): the revisit-magnet guard must still apply.
        let a = e.on_area_entered(&ctx, false);
        assert_eq!(
            a.kind,
            AdvanceKind::OffRoute {
                area_id: ctx.clone()
            },
            "revisiting an already-passed non-town zone must not jump to a later occurrence"
        );
        assert_eq!(e.cursor(), cursor_before, "cursor must not move");
        assert!(a.newly_done.is_empty() && a.newly_skipped.is_empty());
    }

    #[test]
    fn repeated_off_route_event_is_ignored() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true); // active: The Coast group

        let first = e.on_area_entered("1_1_town", false); // off-route
        assert_eq!(
            first.kind,
            AdvanceKind::OffRoute {
                area_id: "1_1_town".into()
            }
        );
        let cursor_before = e.cursor();

        let second = e.on_area_entered("1_1_town", false); // same off-route area again
        assert_eq!(second.kind, AdvanceKind::Ignored);
        assert!(second.newly_done.is_empty() && second.newly_skipped.is_empty());
        assert_eq!(e.cursor(), cursor_before);
        assert_eq!(e.off_route(), Some("1_1_town"));
        assert_eq!(e.active_area(), Some("1_1_2"));
    }

    #[test]
    fn unknown_area_id_is_off_route_not_panic() {
        let mut e = engine();
        let a = e.on_area_entered("totally_bogus", true);
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
            e.on_area_entered(c, true);
            assert!(e.cursor() >= last_cursor, "cursor moved backward at {c}");
            last_cursor = e.cursor();
        }
        assert!(e.is_complete());
        assert_eq!(e.act(), None);
        assert!(e.active_steps().is_empty());
        // After completion further events are Ignored.
        assert_eq!(e.on_area_entered("1_1_1", true).kind, AdvanceKind::Ignored);
    }

    #[test]
    fn next_transition_area_names_the_following_group() {
        let mut e = engine();
        assert_eq!(e.next_transition_area(), Some("1_1_town"));
        e.on_area_entered("1_1_town", true);
        assert_eq!(e.next_transition_area(), Some("1_1_2"));
    }

    #[test]
    fn new_instance_of_a_repeated_forward_zone_advances() {
        // Core of the "follow the player forward" change: a context that
        // has ALREADY been visited (fallen behind the cursor) and recurs
        // later in the route must still trigger the tier-3 jump when the
        // client reports a genuinely NEW zone instance — the player could
        // only get there by actually playing forward through it. A
        // same-instance re-entry (a portal back to the earlier occurrence)
        // must stay governed by the old revisit-magnet guard.
        let e = engine();
        let groups = all_group_starts_and_contexts(&e);

        let mut found: Option<(usize, usize, String)> = None;
        'outer: for gi in 0..groups.len() {
            if e.is_town(&groups[gi].1) {
                continue;
            }
            let min_gj = gi + LOOKAHEAD_GROUPS + 2;
            for gj in min_gj..groups.len() {
                if groups[gi].1 == groups[gj].1 {
                    found = Some((gi, gj, groups[gi].1.clone()));
                    break 'outer;
                }
            }
        }
        let (gi, gj, ctx) = found.expect(
            "expected at least one non-town context to repeat far enough apart in the route; \
             route data may have changed",
        );

        // Direction 1: new_instance = false stays protected (old guard).
        let mut e_same = engine();
        for (_, c) in &groups[0..=gi + 1] {
            e_same.on_area_entered(c, true);
        }
        let cursor_before = e_same.cursor();
        let a_same = e_same.on_area_entered(&ctx, false);
        assert_eq!(
            a_same.kind,
            AdvanceKind::OffRoute {
                area_id: ctx.clone()
            },
            "same-instance revisit of an already-visited forward context must stay off-route"
        );
        assert_eq!(e_same.cursor(), cursor_before);

        // Direction 2: new_instance = true advances (marks prior done),
        // even though the context is already visited and beyond lookahead.
        let mut e_new = engine();
        for (_, c) in &groups[0..=gi + 1] {
            e_new.on_area_entered(c, true);
        }
        let a_new = e_new.on_area_entered(&ctx, true);
        assert_eq!(
            a_new.kind,
            AdvanceKind::Advanced,
            "a genuinely new instance of a forward zone must advance, assuming prior steps done"
        );
        assert_eq!(e_new.active_area(), Some(ctx.as_str()));
        assert_eq!(e_new.cursor(), groups[gj].0);
        assert!(
            !a_new.newly_skipped.is_empty() || !a_new.newly_done.is_empty(),
            "intervening steps must be marked done/skipped"
        );
    }
}
