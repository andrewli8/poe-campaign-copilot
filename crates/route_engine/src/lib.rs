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

/// Where `focus` sits relative to `frontier` (progress). See the `focus`
/// field doc comment on `RouteEngine` for the frontier/focus model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum LocationStatus {
    /// `focus == frontier`: normal forward play.
    OnTrack,
    /// `focus` is behind `frontier`, in a group the route skipped past and
    /// the player has not (per this engine's bookkeeping) actually stood in
    /// before.
    CatchingUp,
    /// `focus` is behind `frontier`, in a group the player has already
    /// stood in before (either it was genuinely completed in sequence, or
    /// this is a repeat behind-detour into the same context).
    Revisiting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum AdvanceKind {
    InPlace,
    Advanced,
    Resumed,
    /// The player walked back into an earlier (behind-`frontier`) route
    /// group. `frontier` and `statuses` are unchanged; only `focus` moves.
    /// `catching_up` mirrors the resulting `LocationStatus` (`true` for
    /// `CatchingUp`, `false` for `Revisiting`).
    Detour {
        catching_up: bool,
    },
    OffRoute {
        area_id: String,
    },
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
    /// Monotonic furthest group reached. Only ever moves forward, via
    /// `apply_forward_advance`. This is "progress" — never un-advances.
    frontier: usize,
    /// The group whose zone the player is physically standing in right
    /// now. Equals `frontier` in normal forward play. Moves BACK to an
    /// earlier (behind-`frontier`) group on a behind detour, and rejoins
    /// `frontier` on the next forward (or in-place) match. This is
    /// "display" — what the composer should render.
    focus: usize,
    /// `focus`'s status relative to `frontier`. Kept in sync with `focus`
    /// on every transition.
    focus_status: LocationStatus,
    off_route: Option<String>,
    /// Area contexts of every step that has fallen behind the frontier,
    /// i.e. zones the route has already visited at least once. Consulted by
    /// the tier-3 distant-forward-jump guard below: many non-town field
    /// zones are "revisit magnets" that recur at multiple points in a
    /// compiled route (e.g. Aspirants' Plaza appears both before and after
    /// the normal Labyrinth). Without this guard, walking back into such a
    /// zone after the route has already passed it would match the LATER
    /// occurrence far down the route and silently teleport the frontier
    /// there, since tier 3 otherwise accepts any forward non-town match.
    ///
    /// This guard only applies to SAME-INSTANCE re-entries (portal back
    /// into a zone you already generated). A genuinely NEW instance of a
    /// forward context (`new_instance == true` on `on_area_entered`, see
    /// `session::SessionEvent::AreaEntered`) bypasses this guard entirely:
    /// a fresh instance can only be reached by actually playing forward, so
    /// it's real progress even if the context happens to repeat or is a
    /// town.
    ///
    /// Deliberately left populated exactly as before the frontier/focus
    /// split (every step the forward-advance marking loop passes over,
    /// Done or Skipped alike, still lands here) so this guard's behavior is
    /// untouched. A behind detour (`apply_behind_detour`) also inserts into
    /// it, which only strengthens the same guard for a later recurrence —
    /// see that function's doc comment.
    visited_contexts: std::collections::HashSet<String>,
    /// Area contexts the player has behind-detoured into at least once
    /// (deliberately separate from `visited_contexts`, which is guard
    /// bookkeeping and gets populated even for groups the route merely
    /// skipped past mechanically). Drives the `CatchingUp` -> `Revisiting`
    /// transition on a second behind-visit to the same context; see
    /// `apply_behind_detour`.
    detoured_contexts: std::collections::HashSet<String>,
}

impl RouteEngine {
    pub fn new(steps: Vec<CompiledStep>, areas: AreaMap) -> Self {
        let statuses = vec![StepStatus::Pending; steps.len()];
        Self {
            steps,
            statuses,
            areas,
            frontier: 0,
            focus: 0,
            focus_status: LocationStatus::OnTrack,
            off_route: None,
            visited_contexts: std::collections::HashSet::new(),
            detoured_contexts: std::collections::HashSet::new(),
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

    /// Back-compat alias for `frontier()`. Progress cursor: the furthest
    /// group reached, monotonic.
    pub fn cursor(&self) -> usize {
        self.frontier
    }

    pub fn steps(&self) -> &[CompiledStep] {
        &self.steps
    }

    pub fn statuses(&self) -> &[StepStatus] {
        &self.statuses
    }

    /// Whether the frontier (progress) has reached the end of the route.
    pub fn is_complete(&self) -> bool {
        self.frontier >= self.steps.len()
    }

    /// Reset the route to its start — a fresh campaign run has begun. Every
    /// step returns to `Pending`, `frontier`/`focus` return to 0, and all
    /// off-route / revisit / detour bookkeeping is cleared. Leaves `steps`
    /// and `areas` (the compiled route itself) intact.
    pub fn restart(&mut self) {
        for s in &mut self.statuses {
            *s = StepStatus::Pending;
        }
        self.frontier = 0;
        self.focus = 0;
        self.focus_status = LocationStatus::OnTrack;
        self.off_route = None;
        self.visited_contexts.clear();
        self.detoured_contexts.clear();
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

    /// Steps of the frontier's (progress) group. See `focus_steps` for the
    /// display-facing equivalent keyed on `focus`.
    pub fn active_steps(&self) -> &[CompiledStep] {
        if self.is_complete() {
            return &[];
        }
        &self.steps[self.frontier..self.group_end(self.frontier)]
    }

    /// The frontier's (progress) area context. See `focus_area` for the
    /// display-facing equivalent keyed on `focus`.
    pub fn active_area(&self) -> Option<&str> {
        self.steps
            .get(self.frontier)
            .map(|s| s.area_context.as_str())
    }

    pub fn off_route(&self) -> Option<&str> {
        self.off_route.as_deref()
    }

    /// The frontier's (progress) act. See `focus_act` for the
    /// display-facing equivalent keyed on `focus`.
    pub fn act(&self) -> Option<u8> {
        self.steps.get(self.frontier).map(|s| s.act)
    }

    /// The area context at `focus` — what the composer should display.
    /// `None` only when `focus` has never diverged from a complete
    /// frontier (i.e. normal play ran the whole route to completion and no
    /// behind detour has happened since).
    pub fn focus_area(&self) -> Option<&str> {
        self.steps.get(self.focus).map(|s| s.area_context.as_str())
    }

    /// Steps of the focus group — what the composer should display.
    pub fn focus_steps(&self) -> &[CompiledStep] {
        if self.focus >= self.steps.len() {
            return &[];
        }
        &self.steps[self.focus..self.group_end(self.focus)]
    }

    /// The act at `focus`.
    pub fn focus_act(&self) -> Option<u8> {
        self.steps.get(self.focus).map(|s| s.act)
    }

    /// `focus`'s status relative to `frontier`.
    pub fn location_status(&self) -> LocationStatus {
        self.focus_status
    }

    /// Number of group starts strictly between `focus` and `frontier` when
    /// `focus` is behind `frontier`; 0 when on track (`focus >= frontier`).
    pub fn groups_behind(&self) -> usize {
        if self.focus >= self.frontier {
            return 0;
        }
        let mut count = 0;
        let mut i = self.group_end(self.focus);
        while i < self.frontier {
            count += 1;
            i = self.group_end(i);
        }
        count
    }

    /// The area context of the group immediately after `focus` — "next
    /// from here". Equals the pre-frontier/focus-split behavior when
    /// `focus == frontier`.
    pub fn next_transition_area(&self) -> Option<&str> {
        if self.focus >= self.steps.len() {
            return None;
        }
        let next = self.group_end(self.focus);
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

    /// Start indices of every group from the frontier onward (index 0 is
    /// always the frontier's own group).
    fn forward_group_starts(&self) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut i = self.frontier;
        while i < self.steps.len() {
            starts.push(i);
            i = self.group_end(i);
        }
        starts
    }

    /// Nearest (largest) group start index strictly behind `frontier` whose
    /// `area_context == area_id`, if any.
    fn behind_group_start(&self, area_id: &str) -> Option<usize> {
        let mut best = None;
        let mut i = 0;
        while i < self.frontier {
            if self.steps[i].area_context == area_id {
                best = Some(i);
            }
            i = self.group_end(i);
        }
        best
    }

    /// Apply a tier-2/tier-3 forward match: advance `frontier` to `target`
    /// with the existing Done/Skipped marking + `visited_contexts` inserts
    /// (unchanged from before the frontier/focus split), then rejoin
    /// `focus` to the new `frontier` and clear any off-route flag.
    fn apply_forward_advance(&mut self, target: usize) -> Advance {
        let mut newly_done = Vec::new();
        let mut newly_skipped = Vec::new();
        let active_end = self.group_end(self.frontier);
        for i in self.frontier..target {
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
        self.frontier = target;

        // Design decision (see plan4-task-1-brief.md "Note on
        // completion"): entering the FINAL group of the route completes
        // the route immediately rather than leaving the engine parked "on"
        // the last group awaiting a further event that would never come
        // (there is nothing beyond the campaign's last step to trigger a
        // subsequent advance). So: if the group we just entered is the
        // last group, mark its steps Done and set frontier = steps.len()
        // now.
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
            self.frontier = last_end;
        }

        self.focus = self.frontier;
        self.focus_status = LocationStatus::OnTrack;
        self.off_route = None;

        Advance {
            kind: AdvanceKind::Advanced,
            newly_done,
            newly_skipped,
        }
    }

    /// Apply a behind detour: move `focus` back to `target` (a group start
    /// strictly behind `frontier`) without touching `frontier` or
    /// `statuses`. `CatchingUp` vs `Revisiting` is decided from two
    /// signals kept deliberately separate from the forward-guard's
    /// `visited_contexts`:
    ///   - `statuses[target]`: `Skipped` means the route only ever
    ///     mechanically skipped past this group (never actually Done), so
    ///     a first behind-visit reads as genuinely "catching up".
    ///   - `detoured_contexts`: whether the player has behind-detoured into
    ///     this exact context before, so a SECOND behind-visit always reads
    ///     `Revisiting` regardless of the underlying step status.
    ///
    /// (Reusing `visited_contexts` alone for this, as a first draft of this
    /// change did, makes `CatchingUp` unreachable: the forward-advance
    /// marking loop above unconditionally inserts every context it passes
    /// — Skipped groups included — so by the time a group is behind
    /// `frontier` at all, its context is necessarily already in
    /// `visited_contexts`.)
    ///
    /// This also inserts `area_id` into `visited_contexts`, reinforcing
    /// (not weakening) the tier-3 revisit-magnet guard: a genuine
    /// behind-visit means the player really has now stood here, so a later
    /// same-instance forward match against a recurrence of this context
    /// should stay guarded exactly as an already-Done group would be. A
    /// legitimate forward jump past it is unaffected, since that path is
    /// separately unlocked by `new_instance`.
    fn apply_behind_detour(&mut self, area_id: &str, target: usize) -> Advance {
        let catching_up = !self.detoured_contexts.contains(area_id)
            && self.statuses[target] == StepStatus::Skipped;

        self.focus = target;
        self.focus_status = if catching_up {
            LocationStatus::CatchingUp
        } else {
            LocationStatus::Revisiting
        };
        self.off_route = None;
        self.detoured_contexts.insert(area_id.to_string());
        self.visited_contexts.insert(area_id.to_string());

        Advance {
            kind: AdvanceKind::Detour { catching_up },
            newly_done: vec![],
            newly_skipped: vec![],
        }
    }

    pub fn on_area_entered(&mut self, area_id: &str, new_instance: bool) -> Advance {
        let no_op = |kind| Advance {
            kind,
            newly_done: vec![],
            newly_skipped: vec![],
        };

        // Tier 1 (idempotent): already standing in this group. Handled
        // ahead of completion/forward/behind checks since it's valid at
        // any frontier position, complete or not, on track or mid-detour.
        if self.focus_area() == Some(area_id) {
            let kind = if self.off_route.take().is_some() {
                AdvanceKind::Resumed
            } else {
                AdvanceKind::InPlace
            };
            return no_op(kind);
        }

        let behind_match = self.behind_group_start(area_id);

        // Once the frontier has reached the end, no further forward
        // progress is possible, but a behind detour into an earlier route
        // group is still meaningful (and displayed).
        if self.is_complete() {
            return match behind_match {
                Some(target) => self.apply_behind_detour(area_id, target),
                None => no_op(AdvanceKind::Ignored),
            };
        }

        // Tier 2/3: nearest forward match from the frontier, gated by the
        // existing guard.
        let starts = self.forward_group_starts();
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
        let forward_matched = starts
            .iter()
            .enumerate()
            .find(|&(_, &s)| self.steps[s].area_context == area_id);

        if let Some((group_offset, &target)) = forward_matched {
            // Tier 2: within lookahead, regardless of town-ness or prior
            // visits (a short back-and-forth is normal play).
            // Tier 3: any forward group, when either (a) it's a non-town
            // area the route has never visited before [existing guard], or
            // (b) the client reports a brand-new zone instance, which is
            // unambiguous forward progress regardless of town-ness or
            // revisit history.
            if group_offset <= LOOKAHEAD_GROUPS || (!is_town && !already_visited) || new_instance {
                return self.apply_forward_advance(target);
            }
        }

        // Behind match: the player has walked back into an earlier route
        // group. Progress (frontier/statuses) is untouched; only focus and
        // its status move.
        if let Some(target) = behind_match {
            return self.apply_behind_detour(area_id, target);
        }

        // Neither a passing forward match nor a behind match: genuinely
        // off route (including the same-instance revisit-magnet case where
        // a forward match exists but fails the guard, and there's no
        // behind occurrence to fall back on).
        if self.off_route.as_deref() == Some(area_id) {
            return no_op(AdvanceKind::Ignored);
        }
        self.off_route = Some(area_id.to_string());
        no_op(AdvanceKind::OffRoute {
            area_id: area_id.to_string(),
        })
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
        assert_eq!(e.focus_area(), Some("1_1_1"));
        assert_eq!(e.act(), Some(1));
        assert_eq!(e.focus_act(), Some(1));
        assert!(!e.active_steps().is_empty());
        assert!(!e.focus_steps().is_empty());
        assert!(!e.is_complete());
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
        assert_eq!(e.groups_behind(), 0);
    }

    #[test]
    fn advances_through_expected_lookahead() {
        let mut e = engine();
        let a = e.on_area_entered("1_1_town", true);
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert_eq!(e.active_area(), Some("1_1_town"));
        assert_eq!(e.focus_area(), Some("1_1_town"));
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
        assert_eq!(e.groups_behind(), 0);
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
        assert_eq!(e.focus_area(), Some("1_1_2"));
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
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
    fn unexpected_non_route_area_is_off_route_and_resumable() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true); // active: The Coast group
        let cursor_before = e.cursor();

        // A zone that never appears anywhere in the route (forward or
        // behind) has nowhere to resolve to and stays off-route.
        let a = e.on_area_entered("totally_bogus_hideout", true);
        assert_eq!(
            a.kind,
            AdvanceKind::OffRoute {
                area_id: "totally_bogus_hideout".into()
            }
        );
        assert_eq!(e.cursor(), cursor_before);
        assert_eq!(e.off_route(), Some("totally_bogus_hideout"));
        assert_eq!(e.active_area(), Some("1_1_2")); // progress untouched
        assert_eq!(e.focus_area(), Some("1_1_2")); // focus untouched

        // Coming back to the focus area resumes.
        let a = e.on_area_entered("1_1_2", false);
        assert_eq!(a.kind, AdvanceKind::Resumed);
        assert_eq!(e.off_route(), None);
    }

    #[test]
    fn distant_never_visited_town_same_instance_is_off_route() {
        // A distant town that the route hasn't reached yet has no behind
        // occurrence to fall back on; the tier-3 guard requires either
        // lookahead proximity, non-town-and-unvisited, or a fresh
        // instance -- none apply here, so this stays off-route (the
        // original revisit-magnet protection intent, even with no behind
        // match available). "1_2_town" (The Forest Encampment, Act 2) is
        // far beyond both lookahead and Act 1 from a fresh engine.
        let mut e = engine();
        assert!(e.is_town("1_2_town"));
        let a = e.on_area_entered("1_2_town", false);
        assert_eq!(
            a.kind,
            AdvanceKind::OffRoute {
                area_id: "1_2_town".into()
            }
        );
        assert_eq!(e.active_area(), Some("1_1_1"));
        assert_eq!(e.focus_area(), Some("1_1_1"));
        assert_eq!(e.cursor(), 0);
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
        assert_eq!(e.focus_area(), Some("1_1_5"));
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
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
    fn revisited_field_zone_beyond_lookahead_becomes_a_behind_detour_not_a_forward_jump() {
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
        // `gi`, so group `gi`'s context falls behind the frontier (Done,
        // via single-group hops) and group `gi + 1` becomes active.
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

        let frontier_before = e.cursor();
        // Portal back to the SAME instance of this zone (not a freshly
        // generated one): the revisit-magnet guard must still prevent a
        // forward jump to the later occurrence -- but under the
        // frontier/focus split it now resolves to a behind detour at the
        // EARLIER occurrence instead of a bare off-route flag.
        let a = e.on_area_entered(&ctx, false);
        assert_eq!(
            a.kind,
            AdvanceKind::Detour { catching_up: false },
            "revisiting an already-passed non-town zone must resolve to its earlier occurrence \
             (Revisiting), never silently jump to the later occurrence"
        );
        assert_eq!(e.cursor(), frontier_before, "frontier must not move");
        assert_ne!(
            e.cursor(),
            groups[gj].0,
            "must not have jumped to the later occurrence"
        );
        assert_eq!(e.focus_area(), Some(ctx.as_str()));
        assert_eq!(e.location_status(), LocationStatus::Revisiting);
        // `gi` is the group immediately behind `gi + 1` here (sequential
        // single-group advance), so groups_behind is 0; the "count > 0"
        // case is covered by behind_skipped_zone_is_catching_up_...
        // (a multi-group distant jump).
        assert_eq!(e.groups_behind(), 0);
    }

    #[test]
    fn repeated_off_route_event_is_ignored() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true); // active: The Coast group

        let first = e.on_area_entered("totally_bogus_repeat", true); // off-route
        assert_eq!(
            first.kind,
            AdvanceKind::OffRoute {
                area_id: "totally_bogus_repeat".into()
            }
        );
        let cursor_before = e.cursor();

        let second = e.on_area_entered("totally_bogus_repeat", false); // same off-route area again
        assert_eq!(second.kind, AdvanceKind::Ignored);
        assert!(second.newly_done.is_empty() && second.newly_skipped.is_empty());
        assert_eq!(e.cursor(), cursor_before);
        assert_eq!(e.off_route(), Some("totally_bogus_repeat"));
        assert_eq!(e.active_area(), Some("1_1_2"));
        assert_eq!(e.focus_area(), Some("1_1_2"));
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
        assert_eq!(e.focus_area(), Some("1_1_1"));
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

        // After completion, a genuine (behind) route area resolves to a
        // detour rather than being flatly ignored -- progress is complete
        // but the player can still be shown where they are.
        let first_ctx = e.steps()[0].area_context.clone();
        let a = e.on_area_entered(&first_ctx, true);
        assert!(matches!(a.kind, AdvanceKind::Detour { .. }));
        assert!(e.is_complete(), "frontier stays complete after a detour");
        assert_eq!(e.focus_area(), Some(first_ctx.as_str()));

        // Only genuinely non-route areas are Ignored after completion.
        assert_eq!(
            e.on_area_entered("totally_bogus_after_complete", true).kind,
            AdvanceKind::Ignored
        );
    }

    #[test]
    fn next_transition_area_names_the_following_group() {
        let mut e = engine();
        assert_eq!(e.next_transition_area(), Some("1_1_town"));
        e.on_area_entered("1_1_town", true);
        assert_eq!(e.next_transition_area(), Some("1_1_2"));
    }

    #[test]
    fn restart_returns_a_completed_route_to_the_start() {
        // `restart()` is the mechanism behind the manual "Reset progress"
        // button. Drive to completion, restart, and confirm the engine is
        // back at a fresh Act 1 start with all steps Pending.
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
        for c in &contexts {
            e.on_area_entered(c, true);
        }
        assert!(e.is_complete());

        e.restart();

        assert!(!e.is_complete());
        assert_eq!(e.cursor(), 0);
        let first = e.steps()[0].area_context.clone();
        assert_eq!(e.active_area(), Some(first.as_str()));
        assert_eq!(e.focus_area(), Some(first.as_str()));
        assert!(e.off_route().is_none());
        assert!(
            e.statuses().iter().all(|&s| s == StepStatus::Pending),
            "all steps return to Pending after restart"
        );
    }

    #[test]
    fn new_instance_of_a_repeated_forward_zone_advances() {
        // Core of the "follow the player forward" change: a context that
        // has ALREADY been visited (fallen behind the frontier) and
        // recurs later in the route must still trigger the tier-3 jump
        // when the client reports a genuinely NEW zone instance — the
        // player could only get there by actually playing forward through
        // it. A same-instance re-entry (a portal back to the earlier
        // occurrence) must stay governed by the old revisit-magnet guard,
        // now resolving to a behind detour rather than a forward jump.
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

        // Direction 1: new_instance = false stays protected (old guard),
        // now resolving to a behind detour instead of a bare off-route.
        let mut e_same = engine();
        for (_, c) in &groups[0..=gi + 1] {
            e_same.on_area_entered(c, true);
        }
        let cursor_before = e_same.cursor();
        let a_same = e_same.on_area_entered(&ctx, false);
        assert_eq!(
            a_same.kind,
            AdvanceKind::Detour { catching_up: false },
            "same-instance revisit of an already-visited forward context must resolve to its \
             earlier occurrence (Revisiting), not silently jump forward"
        );
        assert_eq!(e_same.cursor(), cursor_before);
        assert_eq!(e_same.focus_area(), Some(ctx.as_str()));

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
        assert_eq!(e_new.focus_area(), Some(ctx.as_str()));
        assert_eq!(e_new.location_status(), LocationStatus::OnTrack);
        assert_eq!(e_new.cursor(), groups[gj].0);
        assert!(
            !a_new.newly_skipped.is_empty() || !a_new.newly_done.is_empty(),
            "intervening steps must be marked done/skipped"
        );
    }

    #[test]
    fn behind_skipped_zone_is_catching_up_then_revisiting_on_second_entry() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true);
        let jump = e.on_area_entered("1_1_5", true);
        assert_eq!(jump.kind, AdvanceKind::Advanced);
        let skipped_idx = *jump
            .newly_skipped
            .first()
            .expect("jump should skip at least one group");
        let skipped_ctx = e.steps()[skipped_idx].area_context.clone();
        assert_eq!(e.statuses()[skipped_idx], StepStatus::Skipped);

        let frontier_before = e.cursor();

        // First behind-entry: never actually stood here -- CatchingUp.
        let a = e.on_area_entered(&skipped_ctx, false);
        assert_eq!(a.kind, AdvanceKind::Detour { catching_up: true });
        assert_eq!(e.cursor(), frontier_before, "frontier must stay put");
        assert_eq!(e.focus_area(), Some(skipped_ctx.as_str()));
        assert_eq!(e.location_status(), LocationStatus::CatchingUp);
        assert!(e.groups_behind() > 0);
        assert_eq!(e.off_route(), None);

        // Rejoin the frontier (offset-0 forward match).
        let rejoin = e.on_area_entered("1_1_5", false);
        assert_eq!(rejoin.kind, AdvanceKind::Advanced);
        assert_eq!(
            e.cursor(),
            frontier_before,
            "frontier unchanged by the rejoin"
        );
        assert_eq!(e.focus_area(), Some("1_1_5"));
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
        assert_eq!(e.groups_behind(), 0);

        // Re-entering the same behind zone now reads Revisiting.
        let a2 = e.on_area_entered(&skipped_ctx, false);
        assert_eq!(a2.kind, AdvanceKind::Detour { catching_up: false });
        assert_eq!(e.location_status(), LocationStatus::Revisiting);
        assert_eq!(e.cursor(), frontier_before, "frontier still unchanged");
    }

    #[test]
    fn new_instance_forward_advance_after_detour_rejoins_focus_on_track() {
        let mut e = engine();
        e.on_area_entered("1_1_town", true);
        e.on_area_entered("1_1_2", true);
        let jump = e.on_area_entered("1_1_5", true);
        let skipped_idx = *jump
            .newly_skipped
            .first()
            .expect("jump should skip at least one group");
        let skipped_ctx = e.steps()[skipped_idx].area_context.clone();

        // Detour behind.
        e.on_area_entered(&skipped_ctx, false);
        assert_eq!(e.location_status(), LocationStatus::CatchingUp);
        assert!(e.groups_behind() > 0);

        // Going forward again (a genuinely new instance) must advance the
        // frontier itself and rejoin focus, back to OnTrack.
        let frontier_before = e.cursor();
        let next_ctx = e.steps()[e.group_end(frontier_before)].area_context.clone();
        let a = e.on_area_entered(&next_ctx, true);
        assert_eq!(a.kind, AdvanceKind::Advanced);
        assert!(
            e.cursor() > frontier_before,
            "frontier must advance forward"
        );
        assert_eq!(e.focus_area(), Some(next_ctx.as_str()));
        assert_eq!(e.location_status(), LocationStatus::OnTrack);
        assert_eq!(e.groups_behind(), 0);
        assert_eq!(e.off_route(), None);
    }
}
