# Plan 10: Frontier + Focus Rerouting UX

**Goal:** Decouple "how far I've progressed" (frontier) from "which zone to show guidance for" (focus). Progress only moves forward; the displayed layout/action follows the player wherever they actually are. A zone you skipped and re-enter shows as "Catching up"; a cleared zone you revisit shows as "Revisiting"; only genuinely non-route zones are "off route." A breadcrumb shows how many zones behind your furthest point you are.

**Why:** Today a single monotonic cursor means both progress AND display. Walking back to finish a skipped/missed quest zone shows the wrong (ahead) zone plus an "off route" banner. Separating frontier from focus makes the overlay follow the player without ever un-progressing.

## Model
- **frontier**: monotonic furthest group reached (today's `cursor`). Advances on forward/new-instance entries (existing tier logic). Never decreases.
- **focus**: the group whose zone the player is physically in — what the composer renders. Equals frontier in normal play; moves BACK to a behind group when the player re-enters an earlier route zone (frontier stays put); rejoins frontier when the player goes forward again.
- **location status** (of the focus): `OnTrack` (focus == frontier), `CatchingUp` (focus is a behind group whose context was never actually visited before), `Revisiting` (focus is a behind group already visited/done).
- **off route**: only when the entered zone is on the route NOWHERE reachable as forward-with-guard or behind (e.g. a hideout / unexpected area). Focus stays where it was; the off-route banner shows.

## Task 1: route_engine — frontier/focus split
**Files:** `crates/route_engine/src/lib.rs`

- [ ] Rename the internal progress cursor concept to `frontier` (keep `cursor()` accessor returning frontier for back-compat if other code uses it — check callers). Add `focus: usize` (init 0) and a stored `focus_status: LocationStatus` (init OnTrack). Add `pub enum LocationStatus { OnTrack, CatchingUp, Revisiting }` (Debug, Clone, Copy, PartialEq, Serialize).
- [ ] Add `pub enum` variant `AdvanceKind::Detour { catching_up: bool }` (in addition to existing InPlace/Advanced/Resumed/OffRoute/Ignored).
- [ ] Rewrite `on_area_entered(area_id, new_instance)` with this precedence:
  1. `is_complete()` (frontier at end) AND no behind match for area_id → `Ignored`. (Still allow behind detours after completion — see 4.)
  2. If `focus`'s group context == area_id → clear off_route if set (`Resumed`) else `InPlace`. (No frontier/focus move.)
  3. **Forward match** = nearest group start index ≥ frontier with context == area_id. If it exists AND passes the existing guard (`group_offset <= LOOKAHEAD_GROUPS || (!is_town && !already_visited) || new_instance`): advance `frontier` to it with the existing Done/Skipped marking + `visited_contexts` inserts, set `focus = frontier`, `focus_status = OnTrack`, clear off_route → `Advanced`.
  4. **Behind match** = nearest group start index < frontier with context == area_id (the LARGEST such index, i.e. closest behind). If it exists: set `focus` to it, leave `frontier` and the `statuses` array UNCHANGED (progress intact), clear off_route. `focus_status = if visited_contexts.contains(area_id) { Revisiting } else { CatchingUp }`; then `visited_contexts.insert(area_id)` (so a later re-entry is Revisiting). Return `Detour { catching_up: focus_status == CatchingUp }`.
  5. Else (no forward-passing match, no behind match — including the same-instance-revisit-of-a-repeated-forward-zone case where forward exists but the guard fails and there's no behind occurrence): set `off_route = area_id`, focus/frontier unchanged → `OffRoute` (repeat for same off-route area → `Ignored`).
  - Monotonic frontier preserved (only case 3 moves it, always forward). Idempotency preserved (case 2).
- [ ] Accessors (focus-based for display; keep frontier-based progress ones):
  - `pub fn focus_area(&self) -> Option<&str>` (context at focus; None if complete AND focus==frontier==end).
  - `pub fn focus_steps(&self) -> &[CompiledStep]` (the focus group's steps).
  - `pub fn focus_act(&self) -> Option<u8>`.
  - `pub fn location_status(&self) -> LocationStatus`.
  - `pub fn groups_behind(&self) -> usize` (number of group starts strictly between focus and frontier when focus < frontier, else 0).
  - `pub fn next_transition_area(&self) -> Option<&str>` — CHANGE to the group after `focus` (local "next from here"); equals today's behavior when focus==frontier.
  - Keep `off_route()`, `is_complete()` (frontier-based), `statuses()`, `steps()`, `active_steps()`/`active_area()` — but re-point `active_area`/`active_steps` to FOCUS (they're what the composer uses for display) OR leave them and have the composer switch to focus_* — pick one and update the composer accordingly (prefer: keep active_* meaning frontier, add focus_* for display, composer uses focus_*). Document the choice.
- [ ] Tests: forward progress still advances frontier+focus together (OnTrack); enter a behind skipped-never-visited zone → Detour{catching_up:true}, focus moved back, frontier unchanged, groups_behind>0, location_status CatchingUp; re-enter it → Revisiting; go forward again (new_instance) → frontier advances, focus rejoins, OnTrack, groups_behind 0; a truly-non-route area → OffRoute with focus unchanged; revisit-magnet (same-instance repeated forward zone with a behind occurrence vs none) still protected. Keep all existing route_engine tests green (update signatures/expectations for renamed internals).
- [ ] Gate: `cargo test -p route_engine` + workspace green; fmt; clippy. Commit: `feat: split route frontier from display focus for backtrack UX`

## Task 2: composer + model
**Files:** `crates/composer/src/lib.rs`, `crates/composer/tests/pilot_act1.rs`, `src-tauri/src/pipeline.rs` (if it reads engine accessors), `src/types.ts`

- [ ] `compose` reads FOCUS accessors for display (zone_name, area_id, act, layout images/notes keyed on focus_area, steps_in_zone from focus_steps, primary). Add to `OverlayModel`: `pub location_status: String` ("on_track"|"catching_up"|"revisiting") and `pub groups_behind: u32`. `off_route_zone` unchanged. `route_complete` from is_complete (frontier). next_zone from next_transition_area (now focus-relative).
- [ ] Update `src/types.ts` `OverlayModel`: add `location_status: "on_track" | "catching_up" | "revisiting"` and `groups_behind: number`.
- [ ] Tests: composer test — behind detour (catching-up) produces the behind zone's name/images + location_status "catching_up" + groups_behind>0; revisiting variant; normal play stays "on_track"/0. Update pilot_act1 expectations if the focus change alters any asserted state (it shouldn't for pure-forward fixtures). Keep vitest green (types only there).
- [ ] Gate: workspace + vitest + build green; fmt; clippy. Commit: `feat: surface location status and breadcrumb in overlay model`

## Task 3: frontend chip + breadcrumb
**Files:** `src/FilmstripBar.tsx`, `src/FilmstripBar.css`, `src/FilmstripBar.test.tsx`

- [ ] Render a status chip when `location_status !== "on_track"`: "Catching up" (catching_up) / "Revisiting" (revisiting), styled distinctly (e.g. amber for catching-up, muted for revisiting), near the zone name / header. Not shown in the waiting/complete states.
- [ ] Render a breadcrumb line when `groups_behind > 0`: e.g. "{groups_behind} zone(s) behind your furthest point" (pluralize). Subtle/muted. Also applies in compact mode? Keep compact minimal — show the chip in compact but skip the breadcrumb line there (compact stays one row); show both in full mode.
- [ ] Tests: chip present for catching_up/revisiting and absent for on_track; breadcrumb present when groups_behind>0 with correct pluralization and absent at 0; existing tests still green (extend the shared model() helper defaults: location_status "on_track", groups_behind 0).
- [ ] README: a short "When you backtrack" note under the controls — the overlay follows you to a skipped/earlier zone and labels it, without losing your progress. Humanized, no em/en dashes (grep -c stays 0).
- [ ] Gate: vitest + build + full Rust gate green. Commit: `feat: catching-up/revisiting chip and backtrack breadcrumb`

## Verification
- [ ] Full gate green (Rust + vitest).
- [ ] Behind-zone entry shows that zone with the right chip + breadcrumb; forward entry rejoins frontier; off-route only for non-route zones; revisit-magnet still protected.

## Self-Review Notes
- Frontier stays monotonic (honors the earlier "assume prior done" rule); focus adds the follow-the-player display without un-progressing. `visited_contexts` now also drives CatchingUp vs Revisiting — confirm the tier-3 guard still reads it correctly (a behind detour inserting into visited_contexts must not weaken forward revisit-magnet protection: it only affects `already_visited`, which is already overridden by `new_instance` for genuine forward progress, so a later legit forward jump still works).
- Town/new-instance forward trade-off from Plan-prior remains as-is (out of scope here).
