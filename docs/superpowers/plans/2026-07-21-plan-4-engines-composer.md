# Plan 4: Route Engine, Task Engine, Composer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn compiled route data + live session events into displayable overlay state: a route cursor that follows the player conservatively, a soft pending-task list, and a pure composer producing the model the filmstrip UI will render.

**Architecture:** Three crates per the design spec's module table. `route_engine` holds a cursor over the flattened compiled steps, advancing on `AreaEntered` area ids with a two-tier policy (near lookahead → advance; distant forward non-town match → jump with skips; otherwise → off-route flag, no cursor motion). `task_engine` derives soft pending items (portals, trials, skipped rewards, crafting) from passed steps — no sophisticated inference, per the PRD's explicit guidance. `composer` is a pure function from (engine, tasks, layouts, areas) to an `OverlayModel`, rendering route fragments to text (direction glyphs included) and attaching layout images/notes with audit-staleness flags. A pilot integration test drives the golden Plan 3 fixture end-to-end through session → engines → composer.

**Tech Stack:** Rust, existing crates (`content`, `session`, `event_parser`, `replay`). serde on model types. No new external deps.

## Global Constraints

- Route-engine advancement must be IDEMPOTENT (duplicate `AreaEntered` for the active area is a no-op) and MONOTONIC (the cursor never moves backward; off-route never mutates progress). From Plan 3's final review: name-fallback duplicate events are possible.
- Advancement policy (exact):
  1. If the entered area equals the active group's `area_context` → `InPlace` (clears off-route → `Resumed`).
  2. Else if it matches the `area_context` of one of the next `LOOKAHEAD_GROUPS = 2` upcoming groups → advance there (steps of the active group → `Done`; steps of fully bypassed groups → `Skipped`).
  3. Else if it matches ANY forward group's context AND the entered area is NOT a town → advance to the NEAREST such group (same marking rules). Rationale: field zones are progression evidence; towns are revisit magnets (early logout must not skip half an act).
  4. Else → `OffRoute { area_id }`; cursor unchanged.
- A "group" is a maximal contiguous run of steps sharing `area_context` in the flattened (act-ordered) step list.
- Hideouts/unknown areas never reach the engine (session emits `UnresolvedArea`, not `AreaEntered`) — composer handles display of that case, engines ignore it.
- Task engine is deliberately minimal (PRD 19.3: "Do not build sophisticated inference early"): soft pending items only, no confirmation prompts, no reversibility machinery yet.
- Composer is pure (no I/O, no clocks); all data passed in. Direction glyph table: index 0–7 → `↑ ↗ → ↘ ↓ ↙ ← ↖`.
- Edition 2024; new crates depend only on workspace crates + serde (+ thiserror where errors exist). Commit format `<type>: <description>`, no AI attribution. Controller pushes after each task.
- Layout notes with `audit.status == Corrected` display the `correction` text (falling back to original if correction is None); `Outdated` sets a stale flag; `Unaudited`/`Verified` display as-is, stale=false.

---

### Task 1: `route_engine` crate

**Files:**
- Create: `crates/route_engine/Cargo.toml`, `crates/route_engine/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `content::walk::CompiledStep` (fields: `id, act, section, area_context, fragments, sub_steps`), `content::compile::{ContentPack, compile_route_pack, Variant}` (test-side), `content::game_data::AreaMap` (town lookup for tier 3).
- Produces (all in `route_engine`):
  - `pub const LOOKAHEAD_GROUPS: usize = 2;`
  - `#[derive(Debug, Clone, Copy, PartialEq, Serialize)] pub enum StepStatus { Pending, Done, Skipped }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub enum AdvanceKind { InPlace, Advanced, Resumed, OffRoute { area_id: String }, Ignored }` — `Ignored` = entered area matches nothing forward and engine already complete, or event for the off-route area repeated.
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Advance { pub kind: AdvanceKind, pub newly_done: Vec<usize>, pub newly_skipped: Vec<usize> }`
  - `pub struct RouteEngine { ... }` with:
    - `pub fn new(steps: Vec<CompiledStep>, areas: AreaMap) -> Self`
    - `pub fn from_pack(pack: &ContentPack, areas: AreaMap) -> Self` (flattens `pack.acts` in order)
    - `pub fn on_area_entered(&mut self, area_id: &str) -> Advance`
    - `pub fn cursor(&self) -> usize`
    - `pub fn active_steps(&self) -> &[CompiledStep]` (the active group; empty slice when complete)
    - `pub fn active_area(&self) -> Option<&str>`
    - `pub fn off_route(&self) -> Option<&str>`
    - `pub fn next_transition_area(&self) -> Option<&str>` — the `area_context` of the group after the active one
    - `pub fn act(&self) -> Option<u8>` (act of active step)
    - `pub fn is_complete(&self) -> bool`
    - `pub fn statuses(&self) -> &[StepStatus]`
    - `pub fn steps(&self) -> &[CompiledStep]`

- [ ] **Step 1: Crate scaffold**

`crates/route_engine/Cargo.toml`:

```toml
[package]
name = "route_engine"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
content = { path = "../content" }
```

Add `"crates/route_engine"` to workspace members.

- [ ] **Step 2: Failing tests**

`crates/route_engine/src/lib.rs` — write the struct/enum definitions from the Interfaces block, a stub `on_area_entered` returning `Advance { kind: AdvanceKind::Ignored, newly_done: vec![], newly_skipped: vec![] }`, stub accessors returning defaults, and this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use content::compile::{compile_route_pack, Variant};
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
        assert!(a.newly_done.iter().all(|&i| e.steps()[i].area_context == "1_1_1"));
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
        assert_eq!(a.kind, AdvanceKind::OffRoute { area_id: "1_1_town".into() });
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
        assert!(!a.newly_skipped.is_empty(), "bypassed groups must be Skipped");
        // The formerly active Coast group counts as Done, not Skipped.
        assert!(a.newly_done.iter().any(|&i| e.steps()[i].area_context == "1_1_2"));
    }

    #[test]
    fn unknown_area_id_is_off_route_not_panic() {
        let mut e = engine();
        let a = e.on_area_entered("totally_bogus");
        assert_eq!(a.kind, AdvanceKind::OffRoute { area_id: "totally_bogus".into() });
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
```

Run: `cargo test -p route_engine` — FAIL (stubs).

- [ ] **Step 3: Implement**

Core implementation:

```rust
use content::compile::ContentPack;
use content::game_data::AreaMap;
use content::walk::CompiledStep;
use serde::Serialize;

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
        Self { steps, statuses, areas, cursor: 0, off_route: None }
    }

    pub fn from_pack(pack: &ContentPack, areas: AreaMap) -> Self {
        let steps = pack.acts.iter().flat_map(|a| a.steps.iter().cloned()).collect();
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

    /// Group start indices from the active group onward.
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
        let no_op = |kind| Advance { kind, newly_done: vec![], newly_skipped: vec![] };

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
            .skip(1) // 0 is the active group, handled above
            .find(|(_, &s)| self.steps[s].area_context == area_id);

        let is_town = self.areas.get(area_id).map(|a| a.is_town_area).unwrap_or(false);

        match matched {
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
                Advance { kind: AdvanceKind::Advanced, newly_done, newly_skipped }
            }
            _ => {
                if self.off_route.as_deref() == Some(area_id) {
                    return no_op(AdvanceKind::Ignored);
                }
                self.off_route = Some(area_id.to_string());
                no_op(AdvanceKind::OffRoute { area_id: area_id.to_string() })
            }
        }
    }
}
```

Note on completion: when the cursor advances past the last group, mark remaining active-group steps `Done` — advancing TO a final group then entering nothing more leaves the last group Pending; the full-replay test therefore feeds every context and completion happens when `on_area_entered` advances to the last group and a subsequent... — no: completion in the test asserts after feeding ALL contexts including the last; the last advance lands ON the final group, so `is_complete()` would be false. Resolve by special-casing in the test-driven implementation: after the final group's context is entered and it IS the active group (`InPlace`), the route is "at its last group" — `is_complete()` as defined (`cursor >= len`) will be false. Therefore implement completion as: `pub fn complete_active_group(&mut self)` is NOT added; instead `is_complete()` returns true when the active group is the last AND all its steps... — simplest correct semantics: `is_complete()` = `cursor >= steps.len()`. To make the full-replay test pass, `on_area_entered` for the FINAL group's context, when the final group is already active (InPlace), additionally marks the final group Done and sets `cursor = steps.len()` **only when the final group's context is entered again after being active** — this is fragile. INSTEAD: define the test expectation precisely — after feeding every context once, the engine's active group is the LAST group (kitava kill in act 10); completion is reached by feeding the last context a second time? No. FINAL DECISION (implement exactly this): `on_area_entered` marks the previous groups as described; additionally, when the matched target is the LAST group of the route, the engine marks that group's steps `Done` and sets `cursor = steps.len()` immediately (entering the final zone completes the route — the campaign's last steps are "kill the final boss", unobservable anyway, consistent with soft inference). The full-replay test then passes as written. Document this in a code comment.

Run: `cargo test -p route_engine` — all PASS (adjust only per the documented final-group rule above). fmt + clippy clean; `cargo test --workspace` green.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/route_engine/
git commit -m "feat: route engine with conservative cursor advancement"
```

---

### Task 2: `task_engine` crate

**Files:**
- Create: `crates/task_engine/Cargo.toml`, `crates/task_engine/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `content::walk::CompiledStep`, `content::route_dsl::Fragment`, `route_engine::StepStatus`.
- Produces (in `task_engine`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Serialize)] pub enum PendingKind { Portal, Trial, QuestReward, VendorReward, Crafting }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct PendingTask { pub id: String, pub kind: PendingKind, pub label: String, pub zone_area_id: String, pub step_id: String }`
  - `pub struct TaskEngine { ... }`:
    - `pub fn new() -> Self` (+ `Default`)
    - `pub fn on_step_passed(&mut self, step: &CompiledStep, status: StepStatus)` — scans `step.fragments` **and `step.sub_steps`**:
      - `PortalSet` → push `PendingKind::Portal` ("Portal placed in <area_context>") — regardless of Done/Skipped (unobservable).
      - `PortalUse` → clear all `Portal` pendings (the route consumed the portal).
      - `Trial` → push `Trial` ("Trial of Ascendancy in <area_context>") regardless of status.
      - `Crafting { .. }` → push `Crafting` ("Crafting recipe in <area_context>") only when status == Skipped.
      - `RewardQuest { item }` → push `QuestReward` ("Claim quest reward: <item>") only when Skipped.
      - `RewardVendor { item, .. }` → push `VendorReward` ("Buy from vendor: <item>") only when Skipped.
      - Task id = `"{step_id}:{kind-lowercase}:{ordinal}"` (ordinal = index of this pending among those created from the same step, to keep ids unique).
    - `pub fn pending(&self) -> &[PendingTask]`
    - `pub fn town_reminders(&self) -> Vec<&PendingTask>` — pendings with kind `QuestReward | VendorReward | Crafting`
    - `pub fn pending_count(&self) -> usize`

- [ ] **Step 1: Scaffold + failing tests**

`crates/task_engine/Cargo.toml`:

```toml
[package]
name = "task_engine"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
content = { path = "../content" }
route_engine = { path = "../route_engine" }
```

Add to workspace members. Tests (with stub `on_step_passed` doing nothing):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use content::route_dsl::Fragment;
    use content::walk::CompiledStep;
    use route_engine::StepStatus;

    fn step(id: &str, ctx: &str, frags: Vec<Fragment>) -> CompiledStep {
        CompiledStep {
            id: id.into(),
            act: 1,
            section: "Act 1".into(),
            area_context: ctx.into(),
            fragments: frags,
            sub_steps: vec![],
        }
    }

    #[test]
    fn portal_set_pends_and_portal_use_clears() {
        let mut t = TaskEngine::new();
        t.on_step_passed(
            &step("a1-s001", "1_1_4_1", vec![Fragment::PortalSet]),
            StepStatus::Done,
        );
        assert_eq!(t.pending_count(), 1);
        assert_eq!(t.pending()[0].kind, PendingKind::Portal);
        assert!(t.pending()[0].label.contains("1_1_4_1"));

        t.on_step_passed(
            &step("a1-s005", "1_1_town", vec![Fragment::PortalUse]),
            StepStatus::Done,
        );
        assert_eq!(t.pending_count(), 0);
    }

    #[test]
    fn rewards_pend_only_when_skipped() {
        let mut t = TaskEngine::new();
        let s = step(
            "a1-s010",
            "1_1_town",
            vec![
                Fragment::RewardQuest { item: "Quicksilver Flask".into() },
                Fragment::RewardVendor { item: "Frostblink".into(), cost: None },
            ],
        );
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 0);
        t.on_step_passed(&s, StepStatus::Skipped);
        assert_eq!(t.pending_count(), 2);
        assert_eq!(t.town_reminders().len(), 2);
        assert!(t.pending()[0].label.contains("Quicksilver Flask"));
    }

    #[test]
    fn trial_pends_regardless_of_status_and_ids_are_unique() {
        let mut t = TaskEngine::new();
        let s = step(
            "a2-s020",
            "1_2_4",
            vec![Fragment::Trial, Fragment::PortalSet],
        );
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 2);
        let ids: std::collections::BTreeSet<_> =
            t.pending().iter().map(|p| p.id.clone()).collect();
        assert_eq!(ids.len(), 2);
        // Trials are not town reminders; portals aren't either.
        assert!(t.town_reminders().is_empty());
    }

    #[test]
    fn sub_step_fragments_are_scanned_too() {
        let mut t = TaskEngine::new();
        let mut s = step("a1-s030", "1_1_7_1", vec![]);
        s.sub_steps = vec![vec![Fragment::Trial]];
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 1);
    }

    #[test]
    fn crafting_pends_only_when_skipped() {
        let mut t = TaskEngine::new();
        let s = step("a2-s040", "1_2_5", vec![Fragment::Crafting { area_id: None }]);
        t.on_step_passed(&s, StepStatus::Done);
        assert_eq!(t.pending_count(), 0);
        t.on_step_passed(&s, StepStatus::Skipped);
        assert_eq!(t.pending_count(), 1);
        assert_eq!(t.town_reminders().len(), 1);
    }
}
```

Run: `cargo test -p task_engine` — FAIL.

- [ ] **Step 2: Implement**

```rust
use content::route_dsl::Fragment;
use content::walk::CompiledStep;
use route_engine::StepStatus;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PendingKind {
    Portal,
    Trial,
    QuestReward,
    VendorReward,
    Crafting,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingTask {
    pub id: String,
    pub kind: PendingKind,
    pub label: String,
    pub zone_area_id: String,
    pub step_id: String,
}

#[derive(Default)]
pub struct TaskEngine {
    pending: Vec<PendingTask>,
}

impl TaskEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> &[PendingTask] {
        &self.pending
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn town_reminders(&self) -> Vec<&PendingTask> {
        self.pending
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    PendingKind::QuestReward | PendingKind::VendorReward | PendingKind::Crafting
                )
            })
            .collect()
    }

    pub fn on_step_passed(&mut self, step: &CompiledStep, status: StepStatus) {
        let mut ordinal = 0usize;
        let all_fragments = step
            .fragments
            .iter()
            .chain(step.sub_steps.iter().flatten());

        for fragment in all_fragments {
            let created = match fragment {
                Fragment::PortalSet => Some((
                    PendingKind::Portal,
                    format!("Portal placed in {}", step.area_context),
                )),
                Fragment::PortalUse => {
                    self.pending.retain(|p| p.kind != PendingKind::Portal);
                    None
                }
                Fragment::Trial => Some((
                    PendingKind::Trial,
                    format!("Trial of Ascendancy in {}", step.area_context),
                )),
                Fragment::Crafting { .. } if status == StepStatus::Skipped => Some((
                    PendingKind::Crafting,
                    format!("Crafting recipe in {}", step.area_context),
                )),
                Fragment::RewardQuest { item } if status == StepStatus::Skipped => Some((
                    PendingKind::QuestReward,
                    format!("Claim quest reward: {item}"),
                )),
                Fragment::RewardVendor { item, .. } if status == StepStatus::Skipped => Some((
                    PendingKind::VendorReward,
                    format!("Buy from vendor: {item}"),
                )),
                _ => None,
            };
            if let Some((kind, label)) = created {
                let kind_slug = match kind {
                    PendingKind::Portal => "portal",
                    PendingKind::Trial => "trial",
                    PendingKind::QuestReward => "questreward",
                    PendingKind::VendorReward => "vendorreward",
                    PendingKind::Crafting => "crafting",
                };
                self.pending.push(PendingTask {
                    id: format!("{}:{kind_slug}:{ordinal}", step.id),
                    kind,
                    label,
                    zone_area_id: step.area_context.clone(),
                    step_id: step.id.clone(),
                });
                ordinal += 1;
            }
        }
    }
}
```

Run: `cargo test -p task_engine` — PASS. fmt + clippy + workspace green.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock crates/task_engine/
git commit -m "feat: task engine with soft pending items"
```

---

### Task 3: `composer` crate

**Files:**
- Create: `crates/composer/Cargo.toml`, `crates/composer/src/lib.rs`, `crates/composer/src/render.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: `route_engine::{RouteEngine, StepStatus}`, `task_engine::TaskEngine`, `content::layouts::{LayoutEntry, AuditStatus}`, `content::game_data::AreaMap`, `content::route_dsl::Fragment`.
- Produces:
  - `composer::render::render_fragments(frags: &[Fragment], areas: &AreaMap) -> String` — text fragments verbatim (trimmed at the ends of the final string); `Enter`/`WaypointUse`/`Area` → area display name (fall back to the raw id when unknown); `Waypoint` → "the waypoint"; `WaypointGet` → "waypoint"; `PortalSet` → "place a portal"; `PortalUse` → "take the portal"; `Logout` → "log out"; `Kill{name}`/`Arena{name}`/`QuestText{text}`/`Generic{text}`/`Copy{text}` → their text; `Quest{..}` → "hand in quest"; `RewardQuest{item}` → item; `RewardVendor{item,..}` → "buy <item>"; `Trial` → "Trial of Ascendancy"; `Ascend{..}` → "the Labyrinth"; `Crafting{..}` → "crafting recipe"; `Dir{dir_index}` → the glyph from `↑ ↗ → ↘ ↓ ↙ ← ↖` (index 0–7).
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct NoteView { pub text: String, pub stale: bool }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct OverlayModel { pub zone_name: String, pub area_id: String, pub act: u8, pub off_route_zone: Option<String>, pub layout_images: Vec<String>, pub layout_notes: Vec<NoteView>, pub steps_in_zone: Vec<String>, pub primary: String, pub next_zone: Option<String>, pub pending_count: usize, pub town_reminders: Vec<String>, pub route_complete: bool }`
  - `pub fn compose(engine: &RouteEngine, tasks: &TaskEngine, layouts: &BTreeMap<String, LayoutEntry>, areas: &AreaMap) -> OverlayModel` — pure. Semantics:
    - `zone_name`/`area_id`/`act` from the active group (route_complete=true zeroes: zone_name "Campaign complete", act 0, area_id empty).
    - `off_route_zone` = display name of `engine.off_route()` area (fallback raw id).
    - `layout_images`/`layout_notes` from `layouts.get(active area)`; notes = descriptions then notes, each mapped per the Global Constraints audit rules.
    - `steps_in_zone` = every ACTIVE-group step rendered (skip steps whose rendered text is empty).
    - `primary` = first entry of `steps_in_zone` (or "Continue to <next_zone>" when the group renders empty; or "" when complete).
    - `next_zone` = display name of `engine.next_transition_area()`.
    - `town_reminders` = labels from `tasks.town_reminders()` ONLY when the active area is a town (`areas[...].is_town_area`); empty otherwise.
  - `pub fn layouts_by_area(entries: Vec<LayoutEntry>) -> BTreeMap<String, LayoutEntry>` helper.

- [ ] **Step 1: Scaffold + failing tests**

`crates/composer/Cargo.toml`:

```toml
[package]
name = "composer"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
content = { path = "../content" }
route_engine = { path = "../route_engine" }
task_engine = { path = "../task_engine" }
```

Add to workspace members. Tests in `lib.rs` (stub `compose` returning a default-ish model) and `render.rs`:

`render.rs` tests:

```rust
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
                Fragment::Text { value: "➞ ".into() },
                Fragment::Enter { area_id: "1_1_2".into() },
                Fragment::Text { value: " then ".into() },
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
                Fragment::Kill { name: "Hillock".into() },
                Fragment::Text { value: ", ".into() },
                Fragment::PortalSet,
                Fragment::Text { value: ", ".into() },
                Fragment::Enter { area_id: "bogus".into() },
            ],
            &areas,
        );
        assert_eq!(s, "Hillock, place a portal, bogus");
    }
}
```

`lib.rs` compose tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use content::compile::{compile_route_pack, Variant};
    use content::game_data::load_vendored;
    use content::layouts::load_all_layouts;
    use route_engine::RouteEngine;
    use task_engine::TaskEngine;

    fn fixture() -> (RouteEngine, TaskEngine, std::collections::BTreeMap<String, content::layouts::LayoutEntry>, content::game_data::AreaMap) {
        let (areas, _) = load_vendored().unwrap();
        let pack = compile_route_pack(Variant::LeagueStart).unwrap();
        let engine = RouteEngine::from_pack(&pack, areas.clone());
        let layouts = layouts_by_area(load_all_layouts().unwrap());
        (engine, TaskEngine::new(), layouts, areas)
    }

    #[test]
    fn composes_coast_state_with_real_layout_content() {
        let (mut engine, tasks, layouts, areas) = fixture();
        engine.on_area_entered("1_1_town");
        engine.on_area_entered("1_1_2");

        let m = compose(&engine, &tasks, &layouts, &areas);
        assert_eq!(m.zone_name, "The Coast");
        assert_eq!(m.area_id, "1_1_2");
        assert_eq!(m.act, 1);
        assert!(!m.route_complete);
        assert_eq!(m.off_route_zone, None);
        assert!(!m.layout_images.is_empty(), "The Coast has layout images");
        assert!(!m.layout_notes.is_empty());
        assert!(m.layout_notes.iter().all(|n| !n.stale), "all content is unaudited, not stale");
        assert!(!m.steps_in_zone.is_empty());
        assert_eq!(m.primary, m.steps_in_zone[0]);
        assert!(m.next_zone.is_some());
        assert!(m.town_reminders.is_empty(), "not in town");
    }

    #[test]
    fn off_route_zone_is_reported_with_display_name() {
        let (mut engine, tasks, layouts, areas) = fixture();
        engine.on_area_entered("1_1_town");
        engine.on_area_entered("1_1_2");
        engine.on_area_entered("1_1_town"); // off-route
        let m = compose(&engine, &tasks, &layouts, &areas);
        assert_eq!(m.off_route_zone.as_deref(), Some("Lioneye's Watch"));
        assert_eq!(m.zone_name, "The Coast"); // progress display unchanged
    }

    #[test]
    fn town_reminders_only_in_town() {
        let (mut engine, mut tasks, layouts, areas) = fixture();
        // Manufacture a skipped reward pending.
        let s = content::walk::CompiledStep {
            id: "a1-s099".into(),
            act: 1,
            section: "Act 1".into(),
            area_context: "1_1_town".into(),
            fragments: vec![content::route_dsl::Fragment::RewardQuest {
                item: "Quicksilver Flask".into(),
            }],
            sub_steps: vec![],
        };
        tasks.on_step_passed(&s, route_engine::StepStatus::Skipped);

        let m = compose(&engine, &tasks, &layouts, &areas);
        assert!(m.town_reminders.is_empty(), "Twilight Strand is not a town");
        assert_eq!(m.pending_count, 1);

        engine.on_area_entered("1_1_town");
        let m = compose(&engine, &tasks, &layouts, &areas);
        assert_eq!(m.town_reminders, vec!["Claim quest reward: Quicksilver Flask".to_string()]);
    }
}
```

Run: `cargo test -p composer` — FAIL.

- [ ] **Step 2: Implement `render.rs` and `compose`**

`render.rs`:

```rust
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
                out.push_str(areas.get(area_id).map(|a| a.name.as_str()).unwrap_or(area_id));
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
```

`lib.rs` compose per the Interfaces semantics (layout note mapping):

```rust
fn note_views(entry: &LayoutEntry) -> Vec<NoteView> {
    entry
        .descriptions
        .iter()
        .chain(entry.notes.iter())
        .map(|item| {
            let (text, stale) = match item.audit.status {
                AuditStatus::Corrected => (
                    item.audit.correction.clone().unwrap_or_else(|| item.text.clone()),
                    false,
                ),
                AuditStatus::Outdated => (item.text.clone(), true),
                AuditStatus::Unaudited | AuditStatus::Verified => (item.text.clone(), false),
            };
            NoteView { text, stale }
        })
        .collect()
}
```

(`layout_images` = `entry.images.iter().map(|i| i.file.clone())`.) The rest of `compose` assembles fields exactly per the Interfaces block — every rule there is normative.

Run: `cargo test -p composer` — PASS. fmt + clippy + workspace green.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock crates/composer/
git commit -m "feat: composer producing overlay models from engine state"
```

---

### Task 4: Pilot integration test + README

**Files:**
- Create: `crates/composer/tests/pilot_act1.rs`
- Modify: `README.md`, `crates/composer/Cargo.toml` (dev-deps)

**Interfaces:**
- Consumes: `replay::{fixtures_dir}`, `event_parser::parse_line`, `session::{SessionTracker, SessionEvent}`, plus Task 1–3 crates. Add `replay`, `event_parser`, `session` to `[dev-dependencies]` of `composer`.

- [ ] **Step 1: Write the pilot test**

`crates/composer/tests/pilot_act1.rs`:

```rust
//! End-to-end pilot: golden Client.txt fixture -> session -> route/task
//! engines -> composer, asserting the overlay state at each transition.

use composer::{compose, layouts_by_area};
use content::compile::{compile_route_pack, Variant};
use content::game_data::load_vendored;
use content::layouts::load_all_layouts;
use route_engine::{AdvanceKind, RouteEngine};
use session::{SessionEvent, SessionTracker};
use task_engine::TaskEngine;

#[test]
fn fixture_drives_engines_to_expected_overlay_states() {
    let (areas, _) = load_vendored().unwrap();
    let pack = compile_route_pack(Variant::LeagueStart).unwrap();
    let mut engine = RouteEngine::from_pack(&pack, areas.clone());
    let mut tasks = TaskEngine::new();
    let layouts = layouts_by_area(load_all_layouts().unwrap());
    let mut tracker = SessionTracker::new(areas.clone());

    let text =
        std::fs::read_to_string(replay::fixtures_dir().join("act1-opening.log")).unwrap();

    let mut observed = Vec::new();
    for line in text.lines() {
        for ev in tracker.on_raw(&event_parser::parse_line(line)) {
            if let SessionEvent::AreaEntered { area_id, .. } = &ev {
                let advance = engine.on_area_entered(area_id);
                for &i in advance.newly_done.iter().chain(&advance.newly_skipped) {
                    let status = engine.statuses()[i];
                    tasks.on_step_passed(&engine.steps()[i].clone(), status);
                }
                let m = compose(&engine, &tasks, &layouts, &areas);
                observed.push((
                    area_id.clone(),
                    format!("{:?}", advance.kind),
                    m.zone_name.clone(),
                    m.off_route_zone.is_some(),
                ));
            }
        }
    }

    let compact: Vec<(&str, &str, &str, bool)> = observed
        .iter()
        .map(|(a, k, z, o)| (a.as_str(), k.as_str(), z.as_str(), *o))
        .collect();

    assert_eq!(
        compact,
        vec![
            ("1_1_1", "InPlace", "The Twilight Strand", false),
            ("1_1_town", "Advanced", "Lioneye's Watch", false),
            ("1_1_2", "Advanced", "The Coast", false),
            // Early town revisit: off-route, progress display stays on Coast.
            ("1_1_town", "OffRoute { area_id: \"1_1_town\" }", "The Coast", true),
            ("1_1_2", "Resumed", "The Coast", false),
            ("1_1_3", "Advanced", "The Mud Flats", false),
            // Death + same-instance re-entry: idempotent.
            ("1_1_3", "InPlace", "The Mud Flats", false),
        ]
    );

    // The Coast overlay carried real layout content when active.
    let m = {
        // Recompose at the end: active zone is The Mud Flats.
        compose(&engine, &tasks, &layouts, &areas)
    };
    assert_eq!(m.zone_name, "The Mud Flats");
    assert!(!m.layout_images.is_empty(), "Mud Flats has layout images");
}
```

Note the borrow in the loop: `engine.steps()[i].clone()` before `tasks.on_step_passed` avoids holding an immutable borrow across the mutable call — adjust mechanically if the borrow checker objects (clone the step first, as shown).

- [ ] **Step 2: Run**

`cargo test -p composer --test pilot_act1` — expect PASS. If the observed sequence differs, print it and debug the engine policy against the Global Constraints rules — the expected sequence is derived directly from the fixture and the advancement policy; change it only if you can show the policy itself dictates different output (explain in the report).

- [ ] **Step 3: README + full gate + commit**

Append to README Development section:

```markdown
The pilot test (`cargo test -p composer --test pilot_act1`) drives the full
pipeline — log fixture → session → route/task engines → composer — and is
the quickest way to see the whole system's behavior in one place.
```

Full gate: `cargo test --workspace`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`.

```bash
git add crates/composer/ README.md
git commit -m "test: end-to-end pilot from log fixture to overlay model"
```

---

## Verification (end of plan)

- [ ] Full gate green (workspace tests now span 7 crates).
- [ ] Pilot test passes and its expected sequence matches the fixture-derived derivation.
- [ ] CI green on both OSes after push.

## Self-Review Notes

- Spec coverage: FR-04 route state ✅ (Task 1), FR-06 partial (soft pending states only — Upcoming/Active/Confirmed machinery deferred per PRD 19.3), FR-07 deferred (contextual confirmation needs UI), composer/immutable display model ✅ (Task 3, FR-05 partial: confidence display deferred until audit pass produces non-uniform statuses).
- Plan-3 design notes honored: idempotent advancement (InPlace), monotonic cursor, off-route instead of silent wrong jumps, towns excluded from distant jumps (early-logout case), hideouts never reach engines.
- Type consistency: `RouteEngine`/`StepStatus`/`AdvanceKind` (Task 1) consumed by Tasks 2–4; `TaskEngine`/`PendingKind` (Task 2) by Tasks 3–4; `OverlayModel`/`compose`/`layouts_by_area` (Task 3) by Task 4. `CompiledStep` construction in tests matches content::walk's public fields.
- Known simplification, documented in code: entering the final route group completes the route immediately (final steps are unobservable boss kills).
