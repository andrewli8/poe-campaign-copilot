# Plan 6: PoB Import + Settings

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Path of Building awareness and a real settings surface: paste a PoB share code (or XML), get level-timed build milestones surfaced as town reminders in the overlay; configure the Client.txt path, route variant, and build from a settings window — no env vars.

**Architecture:** A new `pob_import` crate decodes share codes (URL-safe base64 + zlib) and parses PoB XML (quick-xml, manual pulls) into a normalized `LevelingBuildPlan` with per-milestone reliability (Structured / Unsupported per the design spec §10 — Explicit note-parsing deferred). Milestones derive from skill-set title level-ranges and per-gem required levels (vendored gems.json, added to the sync set). `session` gains player pinning (first character to level up = the player) so the pipeline knows the current character level. `composer` gains an optional `BuildContext` producing `build_reminders` (town-gated, level-windowed). The app gains `config.rs` (JSON in the OS config dir), an on-demand settings window (second webview, tray-opened), commands to save config / import PoB / restart the tailer+pipeline, and a React settings page.

**Tech Stack:** quick-xml, flate2, base64 (Rust); tauri-plugin-dialog (native file picker); existing React/vitest stack.

## Global Constraints

- Passive-only: PoB input is a pasted code or a user-picked local file; no network fetches (pobb.in/pastebin URL resolution is explicitly out of scope — reject URLs with a clear message).
- Reliability labels follow spec §10: `Structured` for set-title/level-derived milestones, `Unsupported` when nothing parseable (route-only fallback — the overlay simply shows no build reminders). `Explicit`/`Inferred` are future work; the enum includes all four variants now so the serialized contract is stable.
- Build-reminder rule (normative, composer): reminders shown only in town, only when a player level is known; show milestones with `milestone.level <= player_level + 2` and `milestone.level + 5 >= player_level` (a "recently unlocked / imminent" window), sorted by level, capped at 4. No mutation — composer stays pure.
- Player pinning (normative, session): the first `LevelUp` character observed becomes the pinned player; only the pinned character's level-ups update `player_level`; re-pinning requires a tracker reset (config change restarts the pipeline anyway). Record the single-character assumption in a doc comment.
- Config file: `<app_config_dir>/config.json` — `{ "client_log_path": string|null, "variant": "league-start"|"standard", "pob_code": string|null }`. `POE_COPILOT_LOG`/`POE_COPILOT_LOG_REPLAY` env vars still override at startup (dev path); config-applied tailers always start at end.
- Share-code decode: accept raw PoB codes (URL-safe base64, `-`/`_`), tolerant of missing padding; zlib-inflate; reject anything whose inflated content doesn't start with `<` (clear error). Raw `<PathOfBuilding` XML pasted directly is also accepted.
- Existing behavior must not regress: with no PoB configured, all Plan 5 behavior is byte-identical (compose called with `None` build context).
- Edition 2024; commit format; no AI attribution. Controller pushes after each task.

---

### Task 1: Vendor gems.json + typed loader

**Files:**
- Modify: `tools/sync-exile-leveling.sh` (add `gems.json` to the data file list), `crates/content/src/game_data.rs`, `vendor/exile-leveling/` (new file via script re-run — note: the script clears nothing; it only downloads listed files, and re-downloading at the same pinned REF is byte-identical, verify with `git status`)
- Test: extend the game_data test module

**Interfaces:**
- Produces (in `content::game_data`):
  - `#[derive(Debug, Clone, Deserialize)] pub struct Gem { pub id: String, pub name: String, pub required_level: u8, pub is_support: bool }` (unknown fields ignored)
  - `pub type GemMap = BTreeMap<String, Gem>;` (keyed by metadata id, as in the JSON)
  - `pub fn load_gems(json: &str) -> Result<GemMap, serde_json::Error>`
  - `pub fn load_vendored_gems() -> Result<GemMap, GameDataError>`
  - `pub fn gems_by_name(gems: &GemMap) -> BTreeMap<String, Gem>` (name → gem; on duplicate names keep the LOWEST required_level — vaal/awakened variants share names with different ids)

- [ ] **Step 1:** Add `gems.json` to the `for f in ...` list in the sync script; run `./tools/sync-exile-leveling.sh`; verify `git status` shows ONLY the new `vendor/exile-leveling/data/gems.json` (everything else byte-identical — the pinned REF guarantees it; if anything else changed, STOP and report).
- [ ] **Step 2 (failing test):** extend `crates/content/src/game_data.rs` tests:

```rust
    #[test]
    fn loads_vendored_gems() {
        let gems = load_vendored_gems().expect("gems load");
        assert!(gems.len() > 500);
        let by_name = gems_by_name(&gems);
        let fb = by_name.get("Frostblink").expect("Frostblink exists");
        assert_eq!(fb.required_level, 4);
        assert!(!fb.is_support);
        // Duplicate-name rule: "Fireball" and "Vaal Fireball" are distinct
        // names, but e.g. transfigured/vaal variants sharing a base name must
        // resolve to the lowest required_level.
        assert!(by_name.contains_key("Fireball"));
    }
```

- [ ] **Step 3:** Implement per the Interfaces block (mirror the existing `load_areas`/`load_vendored` patterns; `load_vendored_gems` reads `vendor_dir()/data/gems.json`).
- [ ] **Step 4:** `cargo test -p content` green; fmt/clippy clean. Commit: `feat: vendor gem data with typed loader`

---

### Task 2: `pob_import` crate

**Files:**
- Create: `crates/pob_import/Cargo.toml`, `crates/pob_import/src/lib.rs`, `crates/pob_import/src/decode.rs`, `crates/pob_import/src/parse.rs`, `crates/pob_import/fixtures/leveling-build.xml`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- `crates/pob_import/Cargo.toml` deps: `serde` (derive), `thiserror = "2"`, `base64 = "0.22"`, `flate2 = "1"`, `quick-xml = "0.37"`, `content = { path = "../content" }`.
- Produces (in `pob_import`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Serialize)] #[serde(rename_all = "lowercase")] pub enum Reliability { Explicit, Structured, Inferred, Unsupported }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct GemPlan { pub name: String, pub required_level: Option<u8>, pub is_support: Option<bool>, pub enabled: bool }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct SkillSetPlan { pub title: String, pub level_range: Option<(u16, u16)>, pub gems: Vec<GemPlan> }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Milestone { pub level: u16, pub label: String, pub reliability: Reliability }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct LevelingBuildPlan { pub class_name: String, pub ascend_name: Option<String>, pub skill_sets: Vec<SkillSetPlan>, pub passive_spec_titles: Vec<String>, pub notes: Option<String>, pub milestones: Vec<Milestone>, pub reliability: Reliability }`
  - `pub enum PobError` (thiserror): `NotACode`, `UrlNotSupported`, `Decode(String)`, `Xml(String)`
  - `pub fn decode_share_code(code: &str) -> Result<String, PobError>` — trim; if it starts with `http` → `UrlNotSupported`; if it starts with `<` → return as-is (raw XML passthrough); else base64 URL-safe decode (try padded then unpadded engines) + zlib inflate; result must start with `<` else `Decode`.
  - `pub fn parse_build(xml: &str, gems: &content::game_data::GemMap) -> Result<LevelingBuildPlan, PobError>` — quick-xml pull parse:
    - `<Build className="..." ascendClassName="...">` attrs (ascend "None"/empty → None)
    - `<Skills>` → `<SkillSet title="...">` → `<Skill ...>` → `<Gem nameSpec="..." enabled="true|false">`; skill sets with no `<SkillSet>` wrapper (flat `<Skills><Skill>`) become one set titled "Default"
    - `<Tree>` → `<Spec title="...">` titles
    - `<Notes>` text (trimmed; empty → None)
    - Level-range parsing from set titles: first `A-B` (or `A–B`) numeric pair, else a leading bare number `A` (→ `(A, A)`); no match → None
    - Gem enrichment via `gems_by_name`: fill `required_level`/`is_support` (unknown gem names → None, never an error)
    - Milestones (all `Structured`): (a) each set with a parsed range start L ≥ 2 → `level: L, label: "Switch to skill set '{title}'"`; (b) each unique enabled non-support gem name with known `required_level` ≥ 2 → `level: required_level, label: "Gem available: {name}"` — deduped by name, sorted by level then label
    - Plan `reliability`: `Structured` if any milestone exists, else `Unsupported`
- Fixture `leveling-build.xml`: a hand-written but structurally faithful PoB XML — `<PathOfBuilding>` root; Build className="Ranger" ascendClassName="Deadeye"; two SkillSets titled `"1-12"` (Gems: Caustic Arrow enabled, Pierce enabled) and `"13-32"` (Gems: Toxic Rain enabled, Mirage Archer enabled, Frostblink enabled); a Tree with Specs titled "Level 20" and "Level 45"; Notes "Rush Merveil. Buy Toxic Rain at 12."

- [ ] **Step 1 (failing tests):** in `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use content::game_data::load_vendored_gems;

    fn fixture_xml() -> String {
        std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/leveling-build.xml"),
        )
        .unwrap()
    }

    fn encode(xml: &str) -> String {
        use base64::Engine as _;
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write as _;
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(xml.as_bytes()).unwrap();
        base64::engine::general_purpose::URL_SAFE.encode(e.finish().unwrap())
    }

    #[test]
    fn share_code_roundtrip_and_xml_passthrough() {
        let xml = fixture_xml();
        assert_eq!(decode_share_code(&encode(&xml)).unwrap(), xml);
        assert_eq!(decode_share_code(&xml).unwrap(), xml);
        assert!(matches!(
            decode_share_code("https://pobb.in/abc"),
            Err(PobError::UrlNotSupported)
        ));
        assert!(decode_share_code("!!!not-base64!!!").is_err());
    }

    #[test]
    fn parses_fixture_into_plan() {
        let gems = load_vendored_gems().unwrap();
        let plan = parse_build(&fixture_xml(), &gems).unwrap();
        assert_eq!(plan.class_name, "Ranger");
        assert_eq!(plan.ascend_name.as_deref(), Some("Deadeye"));
        assert_eq!(plan.skill_sets.len(), 2);
        assert_eq!(plan.skill_sets[0].level_range, Some((1, 12)));
        assert_eq!(plan.skill_sets[1].level_range, Some((13, 32)));
        assert_eq!(plan.passive_spec_titles, vec!["Level 20", "Level 45"]);
        assert!(plan.notes.as_deref().unwrap().contains("Toxic Rain"));
        assert_eq!(plan.reliability, Reliability::Structured);

        // Milestones: set switch at 13, plus gem availabilities (e.g.
        // Frostblink at its vendored required_level 4).
        assert!(plan
            .milestones
            .iter()
            .any(|m| m.level == 13 && m.label.contains("13-32")));
        assert!(plan
            .milestones
            .iter()
            .any(|m| m.label == "Gem available: Frostblink" && m.level == 4));
        // Support gems excluded from gem milestones.
        assert!(!plan.milestones.iter().any(|m| m.label.contains("Mirage Archer")));
        // Sorted by level.
        let levels: Vec<u16> = plan.milestones.iter().map(|m| m.level).collect();
        let mut sorted = levels.clone();
        sorted.sort();
        assert_eq!(levels, sorted);
    }

    #[test]
    fn unparseable_build_is_unsupported_not_error() {
        let gems = load_vendored_gems().unwrap();
        let plan = parse_build(
            "<PathOfBuilding><Build className=\"Witch\"/></PathOfBuilding>",
            &gems,
        )
        .unwrap();
        assert_eq!(plan.reliability, Reliability::Unsupported);
        assert!(plan.milestones.is_empty());
    }
}
```

(If the fixture's gem names have vendored `required_level`s that differ from the test's expectations, verify against gems.json and adjust the TEST constants to the vendored truth — vendored data is the source of truth. Mirage Archer's exclusion depends on `is_support: true` in gems.json — verify; if it isn't marked support there, swap the fixture's support-gem example for one that is, e.g. "Pierce", and adjust assertions.)

- [ ] **Step 2:** RED → implement `decode.rs`/`parse.rs`/`lib.rs` re-exports → GREEN. quick-xml parsing notes: use `Reader::from_str`, loop on `read_event()`, match `Event::Start`/`Event::Empty` by tag name (`Build`, `SkillSet`, `Skill`, `Gem`, `Spec`, `Notes`), read attributes with `attr.unescape_value()`; `Notes` body via `Event::Text` while inside the element. Track a small state (in-skills / current set). Flat `<Skills>` without `SkillSet` → synthesize "Default" set on first `<Skill>`.
- [ ] **Step 3:** fmt/clippy/workspace green. Commit: `feat: pob_import crate decoding share codes into leveling plans`

---

### Task 3: Session player pinning

**Files:**
- Modify: `crates/session/src/lib.rs`

**Interfaces:**
- `SessionTracker` gains: `pub fn pinned_character(&self) -> Option<&str>`, `pub fn player_level(&self) -> Option<u16>`.
- Pinning rule (normative, from Global Constraints): first LevelUp character pins; only pinned character's LevelUps update `player_level`. Doc comment records the single-character-session assumption and that config changes rebuild the tracker.

- [ ] **Step 1 (failing test):**

```rust
    #[test]
    fn pins_first_leveling_character_and_tracks_their_level() {
        let (mut t, _) = {
            let (areas, _) = load_vendored().unwrap();
            (SessionTracker::new(areas), ())
        };
        assert_eq!(t.player_level(), None);
        t.on_raw(&RawEvent::LevelUp {
            character: "Me".into(), class: "Ranger".into(), level: 5, at: "t".into(),
        });
        assert_eq!(t.pinned_character(), Some("Me"));
        assert_eq!(t.player_level(), Some(5));
        // A party member levels: ignored.
        t.on_raw(&RawEvent::LevelUp {
            character: "Friend".into(), class: "Witch".into(), level: 40, at: "t".into(),
        });
        assert_eq!(t.player_level(), Some(5));
        t.on_raw(&RawEvent::LevelUp {
            character: "Me".into(), class: "Ranger".into(), level: 6, at: "t".into(),
        });
        assert_eq!(t.player_level(), Some(6));
    }
```

- [ ] **Step 2:** RED → implement (two fields + update in the LevelUp arm; LevelUp SessionEvents still pass through unchanged) → GREEN; fmt/clippy/workspace green. Commit: `feat: pin the player character from the first level-up`

---

### Task 4: Composer build reminders

**Files:**
- Modify: `crates/composer/src/lib.rs`, `crates/composer/Cargo.toml` (add `pob_import` dep), callers: `crates/composer/tests/pilot_act1.rs`, `src-tauri/src/pipeline.rs` (pass `None` for now — Task 5 threads the real value)

**Interfaces:**
- `pub struct BuildContext<'a> { pub plan: &'a pob_import::LevelingBuildPlan, pub player_level: Option<u16> }`
- `compose` gains a final parameter `build: Option<BuildContext<'_>>`.
- `OverlayModel` gains `pub build_reminders: Vec<String>` — populated per the normative Global-Constraints rule (town-only, level window `[level, level+5]` looking back / `level >= milestone.level - 2` looking forward — i.e. show milestone m when `m.level <= player_level + 2 && m.level + 5 >= player_level`, sorted by level, cap 4). Empty when: not in town, no build, no player level, or plan reliability is `Unsupported`.
- `src/types.ts` + FilmstripBar: `build_reminders: string[]` rendered in the text block (distinct styling class `build`), with a component test. (Frontend change included HERE to keep the contract change atomic.)

- [ ] **Step 1 (failing tests):** composer test using a hand-built plan:

```rust
    #[test]
    fn build_reminders_are_town_gated_and_level_windowed() {
        let (mut engine, tasks, layouts, areas) = fixture();
        let plan = pob_import::LevelingBuildPlan {
            class_name: "Ranger".into(),
            ascend_name: None,
            skill_sets: vec![],
            passive_spec_titles: vec![],
            notes: None,
            milestones: vec![
                pob_import::Milestone { level: 4, label: "Gem available: Frostblink".into(), reliability: pob_import::Reliability::Structured },
                pob_import::Milestone { level: 12, label: "Gem available: Toxic Rain".into(), reliability: pob_import::Reliability::Structured },
                pob_import::Milestone { level: 2, label: "Old".into(), reliability: pob_import::Reliability::Structured },
            ],
            reliability: pob_import::Reliability::Structured,
        };
        let ctx = |lvl| Some(BuildContext { plan: &plan, player_level: Some(lvl) });

        // Not in town: no reminders even with a build.
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(5));
        assert!(m.build_reminders.is_empty());

        engine.on_area_entered("1_1_town");
        // Level 5: Frostblink (4) is within [3,7] window; Toxic Rain (12) is not; "Old" (2) has aged out (2+5 < ... adjust per rule).
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(5));
        assert_eq!(m.build_reminders, vec!["Gem available: Frostblink".to_string()]);
        // Level 10: Toxic Rain (12) now within +2 lookahead; Frostblink (4+5=9 < 10) aged out.
        let m = compose(&engine, &tasks, &layouts, &areas, ctx(10));
        assert_eq!(m.build_reminders, vec!["Gem available: Toxic Rain".to_string()]);
        // No player level: nothing.
        let m = compose(&engine, &tasks, &layouts, &areas, Some(BuildContext { plan: &plan, player_level: None }));
        assert!(m.build_reminders.is_empty());
        // No build: nothing (and everything else unchanged).
        let m = compose(&engine, &tasks, &layouts, &areas, None);
        assert!(m.build_reminders.is_empty());
    }
```

Verify the window arithmetic in the test comments against the normative rule before implementing; the rule text governs. Frontend test: FilmstripBar shows `build_reminders` entries with class `build`.

- [ ] **Step 2:** RED → implement (compose signature change; update ALL callers: existing composer tests pass `None`, pilot test passes `None`, pipeline passes `None` for now) → GREEN. `npm test` updated + green; `cargo test --workspace` green; fmt/clippy clean.
- [ ] **Step 3:** Commit: `feat: build reminders in overlay model and filmstrip`

---

### Task 5: App config + settings backend

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/main.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/Cargo.toml` (+ `tauri-plugin-dialog = "2"`, `pob_import` path dep), `package.json` (+ `"@tauri-apps/plugin-dialog": "^2"`), `src-tauri/capabilities/default.json`, `src-tauri/tauri.conf.json` (nothing needed for dynamic windows — settings window is created at runtime)

**Interfaces:**
- `config.rs`: `#[derive(Debug, Clone, Serialize, Deserialize, Default)] pub struct AppConfig { pub client_log_path: Option<String>, #[serde(default = "default_variant")] pub variant: String, pub pob_code: Option<String> }` (`default_variant()` → `"league-start"`); `pub fn load(app: &tauri::AppHandle) -> AppConfig` (missing/corrupt file → Default, log to stderr); `pub fn save(app: &tauri::AppHandle, cfg: &AppConfig) -> Result<(), String>` — path `app.path().app_config_dir()?/config.json`, create dirs.
- `pipeline.rs`: `Pipeline::new(variant: content::compile::Variant, build: Option<pob_import::LevelingBuildPlan>) -> Result<...>`; stores the plan; `current_model` passes `BuildContext { plan, player_level: tracker.player_level() }` when a plan exists. `UiModel` gains `pub build_summary: Option<String>` ("Ranger (Deadeye) — N milestones") for display.
- Commands (all returning `Result<_, String>` where fallible):
  - `get_config() -> AppConfig`
  - `pick_log_file() -> Option<String>` — `tauri_plugin_dialog::DialogExt` blocking file picker
  - `import_pob(code: String) -> Result<PobSummary, String>` where `#[derive(Serialize)] struct PobSummary { class_name: String, ascend_name: Option<String>, milestone_count: usize, reliability: String }` — decode+parse WITHOUT saving (preview)
  - `apply_settings(cfg: AppConfig) -> Result<(), String>` — validate (pob code parses if present; log path exists if present), `config::save`, rebuild `Pipeline` (variant + parsed plan), stop the old tailer (`TailerHandle` stored in state as `Mutex<Option<TailerHandle>>`; `take()` then `stop()`), spawn a new tailer at the configured path (start_at_end = true), emit a fresh `overlay-model`
  - `open_settings()` — creates (or focuses if exists) a `settings` WebviewWindow: `WebviewUrl::App("index.html?window=settings".into())`, 560×680, decorated, resizable, focused. Also add a tray item "Settings…" doing the same.
- Startup order: env `POE_COPILOT_LOG` overrides config's `client_log_path` (dev); otherwise config path; neither → no tailer (waiting state).
- Capabilities: add `"settings"` to `windows`, add `"dialog:allow-open"` (adjust to the actual permission id registry as before).

- [ ] **Step 1 (failing tests where testable):** pipeline unit tests updated for the new constructor (pass `Variant::LeagueStart, None`); add one test constructing a Pipeline WITH a plan (hand-built like Task 4's) + a pinned level (feed a LevelUp line then town entry lines) asserting `build_summary` is Some and town model contains a build reminder. Config load/save round-trip is app-handle-dependent — extract the pure parts (`fn parse_config(json: &str) -> AppConfig` with serde defaults; `fn config_json(cfg) -> String`) and unit-test those (corrupt json → Default).
- [ ] **Step 2:** Implement; `npm install`; full Rust gate green; `npm run build` green.
- [ ] **Step 3:** Smoke: launch `npm run tauri dev` briefly — tray now has Settings…; invoke can't be manually clicked headlessly, so verify via logs + `cargo check`; kill.
- [ ] **Step 4:** Commit: `feat: app config, settings backend, and pipeline rebuild wiring`

---

### Task 6: Settings frontend + docs

**Files:**
- Create: `src/SettingsPage.tsx`, `src/SettingsPage.css`, `src/SettingsPage.test.tsx`
- Modify: `src/main.tsx` (route by `?window=settings` URL param), `src/types.ts` (AppConfig, PobSummary, build_summary/build_reminders already from Task 4), `README.md`

**Interfaces:**
- `SettingsPage` (presentational core + a thin container): props `{ config: AppConfig; onPick: () => void; onImportPreview: (code: string) => void; preview: PobSummary | null; previewError: string | null; onSave: (cfg: AppConfig) => void; saving: boolean; savedAt: number | null }`. Elements: log-path row (current path or "not set", Browse button), variant select (league-start / standard), PoB textarea + "Preview import" button + preview card (class/ascend, milestone count, reliability badge) or error text, Save button, saved confirmation. Container wires commands via `invoke` (untested, like useOverlay).
- `main.tsx`: `new URLSearchParams(location.search).get("window") === "settings"` → render settings container; else overlay App.
- Tests (5+): renders config values; Browse calls onPick; preview card renders summary incl. reliability badge; preview error renders; Save passes edited config (variant change + textarea content).
- README: settings usage section (tray → Settings…, pick Client.txt, paste PoB code, Save) replacing/augmenting the env-var docs (env vars remain documented as dev overrides).

- [ ] **Step 1:** Failing vitest tests → RED.
- [ ] **Step 2:** Implement page + container + routing; styling consistent with the overlay aesthetic (dark, gold accents) but as a normal opaque window (background #16110c, normal text sizes).
- [ ] **Step 3:** Full gate: `cargo test --workspace`, fmt, clippy, `npm test`, `npm run build`. Live smoke: launch, open Settings via tray if possible (or via `open_settings` invoked from a temporary console eval — optional), verify window opens and renders; kill. Document what was verified.
- [ ] **Step 4:** README update. Commit: `feat: settings window for log path, route variant, and PoB import`

---

## Verification (end of plan)

- [ ] Full gate green (Rust + vitest; new crates included).
- [ ] Pipeline-with-build test proves: LevelUp pin + town entry → build reminder appears in the model.
- [ ] Settings window opens from tray; config round-trips; PoB preview works with a real share code (paste any public build's code manually if available — otherwise the fixture roundtrip stands).
- [ ] CI green after push.

## Self-Review Notes

- Spec §10 coverage: share code + XML input ✅, normalized LevelingBuildPlan ✅, reliability classes (Structured/Unsupported live; Explicit/Inferred enum-present, derivation deferred — recorded), route-only fallback ✅ (Unsupported → no reminders). FR-01 log discovery ✅ (picker + persistence). FR-08 ✅ (decode + normalize). FR-09 town-preferred reminders ✅ (town-gated by rule).
- Contract stability: compose signature change is the one breaking internal change; all callers updated in Task 4 atomically. TS mirrors extended in the same commits as their Rust counterparts.
- Deferred: Explicit note-parsing milestones, passive-tree snapshot reminders, URL fetching (never), multi-character pinning override UI, party-member guild-tag verification (Windows session).
