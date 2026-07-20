# Plan 1: Foundation & Route Data Pipeline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the public repo (Rust workspace + Tauri 2 app + CI) and build the exile-leveling route pipeline: vendored data, DSL parser, stateful route walk, and a content-pack compiler that emits route JSON for all 10 acts.

**Architecture:** A `content` Rust crate owns route-DSL parsing (pure syntax), game-data loading (areas/quests JSON), and a stateful walk that resolves each route step's "area context" (which zone the player is in when the step applies). A `compile-content` binary emits versioned content-pack JSON. A minimal Tauri 2 + React app scaffold proves the desktop shell builds on macOS. Spec: `docs/superpowers/specs/2026-07-20-poe-campaign-copilot-design.md`.

**Tech Stack:** Rust stable (edition 2024), serde/serde_json/thiserror, Tauri 2, React 18 + TypeScript + Vite, GitHub Actions (macOS + Windows).

## Global Constraints

- Passive-only: nothing in this repo may read game memory, simulate input, or make network calls during a play session. This plan touches no game APIs at all.
- English-only game clients (affects later plans; no localization scaffolding here).
- Code license: MIT. Vendored exile-leveling data: MIT, attributed. `CREDITS.md` must exist from the first commit.
- Rust edition 2024, `resolver = "2"`. Crate deps for `content`: serde, serde_json, thiserror only.
- Commit format: `<type>: <description>` (feat/fix/refactor/docs/test/chore/ci). No AI attribution lines in commit messages.
- Vendored exile-leveling data is pinned to commit `1961ef839235e831ee61413de331cce42eb78f24`.
- All commands run from the repo root `/Users/andrew/Desktop/projects/poe-campaign-copilot` unless stated otherwise.

## Plan Series

This is Plan 1 of ~7. Later plans (written after this executes): docx layout extraction; log tailer + replay harness; route/task engines + composer; filmstrip overlay UI; PoB import + settings; content audit + Windows validation.

---

### Task 1: Repo scaffold and `content` crate skeleton

**Files:**
- Create: `LICENSE`, `CREDITS.md`, `README.md`, `rust-toolchain.toml`, `Cargo.toml`, `crates/content/Cargo.toml`, `crates/content/src/lib.rs`
- Modify: `.gitignore`

**Interfaces:**
- Produces: cargo workspace with member `crates/content`; crate name `content`.

- [ ] **Step 1: Write repo meta files**

`LICENSE` — the standard MIT license text, copyright line:

```
MIT License

Copyright (c) 2026 Andrew Li

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

`CREDITS.md`:

```markdown
# Credits & Attribution

PoE Campaign Copilot is an unaffiliated fan tool. Path of Exile is a
registered trademark of Grinding Gear Games.

- **Route data** — vendored from
  [HeartofPhos/exile-leveling](https://github.com/HeartofPhos/exile-leveling)
  (MIT). See `vendor/exile-leveling/LICENSE` and `VENDOR.md` for the pinned
  commit.
- **Zone layout images** — created by **Engineering Eternity**
  (https://www.youtube.com/@EngineeringEternity), included with credit from a
  community-shared layout compilation. If you are the rights holder and want
  anything removed, open an issue and it will be taken down promptly.
- **Zone layout notes** — adapted from a community "PoE Map Layouts" cheat
  sheet compilation (author's notes based on Engineering Eternity's videos).
```

`README.md`:

```markdown
# PoE Campaign Copilot

A transparent Windows overlay for Path of Exile 1 campaign leveling
(Acts 1–10). Shows the current route step, zone layout images, and
build reminders — passively, from Client.txt only.

**Status: early development.**

- Passive-only: reads the game's `Client.txt` log. Never touches game
  memory, never simulates input, no network during play.
- English game clients only (for now).
- Route data based on [exile-leveling](https://github.com/HeartofPhos/exile-leveling) (MIT).
- Layout images by Engineering Eternity — see [CREDITS.md](CREDITS.md).

## Development

Rust workspace + Tauri 2. Built on macOS, validated on Windows.

    cargo test --workspace

License: MIT (code). See CREDITS.md for third-party content.
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
```

`Cargo.toml` (workspace root):

```toml
[workspace]
resolver = "2"
members = ["crates/content"]
```

Append to existing `.gitignore` (keep existing lines):

```
content-pack/
.DS_Store
```

- [ ] **Step 2: Create the `content` crate**

`crates/content/Cargo.toml`:

```toml
[package]
name = "content"
version = "0.1.0"
edition = "2024"
license = "MIT"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
```

`crates/content/src/lib.rs`:

```rust
//! Content pipeline: route DSL parsing, game data, and content-pack
//! compilation for PoE Campaign Copilot.

pub fn crate_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() {
        assert!(super::crate_ready());
    }
}
```

- [ ] **Step 3: Run the smoke test**

Run: `cargo test --workspace`
Expected: `test tests::workspace_builds ... ok`, 1 passed.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: scaffold workspace, content crate, license and credits"
```

---

### Task 2: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: CI running fmt, clippy, tests on macOS + Windows. Task 10 appends a frontend job.

- [ ] **Step 1: Write the workflow**

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 2: Verify the checks pass locally**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: no fmt diffs, no clippy warnings, tests pass.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt, clippy, test workflow for macOS and Windows"
```

---

### Task 3: Vendor exile-leveling route data

**Files:**
- Create: `tools/sync-exile-leveling.sh`, `vendor/exile-leveling/` (10 route files, 3 JSON files, LICENSE, VENDOR.md)

**Interfaces:**
- Produces: `vendor/exile-leveling/routes/act-{1..10}.txt`, `vendor/exile-leveling/data/{areas,quests,kill-waypoints}.json` — consumed by Tasks 6–9 tests via `CARGO_MANIFEST_DIR/../../vendor/exile-leveling`.

- [ ] **Step 1: Write the sync script**

`tools/sync-exile-leveling.sh`:

```bash
#!/usr/bin/env bash
# Vendors route + game data from HeartofPhos/exile-leveling (MIT).
# Bump REF to resync after a new league; then run this script and commit.
set -euo pipefail

REF="1961ef839235e831ee61413de331cce42eb78f24"
BASE="https://raw.githubusercontent.com/HeartofPhos/exile-leveling/${REF}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/vendor/exile-leveling"

mkdir -p "${DEST}/routes" "${DEST}/data"

for act in 1 2 3 4 5 6 7 8 9 10; do
  curl -fsSL "${BASE}/common/data/routes/act-${act}.txt" \
    -o "${DEST}/routes/act-${act}.txt"
done

for f in areas.json quests.json kill-waypoints.json; do
  curl -fsSL "${BASE}/common/data/json/${f}" -o "${DEST}/data/${f}"
done

curl -fsSL "${BASE}/LICENSE" -o "${DEST}/LICENSE"

printf 'Vendored from https://github.com/HeartofPhos/exile-leveling\ncommit: %s\nsynced: %s\n' \
  "${REF}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "${DEST}/VENDOR.md"

echo "Synced exile-leveling data at ${REF} into ${DEST}"
```

- [ ] **Step 2: Run it and inspect**

Run: `chmod +x tools/sync-exile-leveling.sh && ./tools/sync-exile-leveling.sh && ls vendor/exile-leveling/routes | wc -l`
Expected: `Synced exile-leveling data ...` and `10`.

- [ ] **Step 3: Commit**

```bash
git add tools/ vendor/
git commit -m "feat: vendor exile-leveling route and game data (MIT, pinned)"
```

---

### Task 4: Fragment model and line-level fragment parser

The DSL embeds `{kind|param|param}` fragments in free text. Known kinds and arities (from exile-leveling's `fragment/index.ts`, vendored ref above): `kill|text`, `arena|text`, `area|area_id`, `enter|area_id`, `logout` (0 params), `waypoint` (0 params = "a waypoint is here"; 1 param = travel to area), `waypoint_get` (0), `portal|set`, `portal|use`, `quest|quest_id[|reward_offer_id...]`, `quest_text|text`, `generic|text`, `reward_quest|item`, `reward_vendor|item[|cost]`, `trial` (0), `ascend|version`, `crafting[|area_id]`, `dir|degrees` (degrees mod 360 must be a multiple of 45; stored as index 0–7 where 0=up/north, going clockwise), `copy|text...` (params joined). `#` starts a comment for the rest of the line. Text between fragments is kept verbatim.

**Files:**
- Create: `crates/content/src/route_dsl/mod.rs`, `crates/content/src/route_dsl/fragment.rs`
- Modify: `crates/content/src/lib.rs`
- Test: inline `#[cfg(test)]` in `fragment.rs`

**Interfaces:**
- Produces:
  - `content::route_dsl::Fragment` — enum, variants exactly: `Text { value: String }`, `Kill { name: String }`, `Arena { name: String }`, `Area { area_id: String }`, `Enter { area_id: String }`, `Logout`, `Waypoint`, `WaypointUse { area_id: String }`, `WaypointGet`, `PortalSet`, `PortalUse`, `Quest { quest_id: String, reward_offer_ids: Vec<String> }`, `QuestText { text: String }`, `Generic { text: String }`, `RewardQuest { item: String }`, `RewardVendor { item: String, cost: Option<String> }`, `Trial`, `Ascend { version: String }`, `Crafting { area_id: Option<String> }`, `Dir { dir_index: u8 }`, `Copy { text: String }`. Derives `Debug, Clone, PartialEq, serde::Serialize`, serde `tag = "type", rename_all = "snake_case"`.
  - `content::route_dsl::FragmentError` — enum with `UnknownKind { line: usize, kind: String }`, `InvalidArity { line: usize, kind: String }`, `Unterminated { line: usize }`, `InvalidDir { line: usize, value: String }`.
  - `pub fn parse_fragments(text: &str, line_no: usize) -> Result<Vec<Fragment>, FragmentError>`

- [ ] **Step 1: Write the failing tests**

In `crates/content/src/route_dsl/fragment.rs` (tests at bottom of the file you are about to create — write the test module first, with a stub `parse_fragments` returning `Ok(vec![])` so it compiles):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enter_line_with_arrow_text_and_comment() {
        let frags = parse_fragments("➞ {enter|1_1_2} #The Coast", 1).unwrap();
        assert_eq!(
            frags,
            vec![
                Fragment::Text { value: "➞ ".into() },
                Fragment::Enter { area_id: "1_1_2".into() },
            ]
        );
    }

    #[test]
    fn parses_mixed_text_and_multiple_fragments() {
        let frags =
            parse_fragments("Find and kill {kill|Hailrake}, take {quest_text|Medicine Chest}", 3)
                .unwrap();
        assert_eq!(
            frags,
            vec![
                Fragment::Text { value: "Find and kill ".into() },
                Fragment::Kill { name: "Hailrake".into() },
                Fragment::Text { value: ", take ".into() },
                Fragment::QuestText { text: "Medicine Chest".into() },
            ]
        );
    }

    #[test]
    fn waypoint_arity_selects_variant() {
        assert_eq!(parse_fragments("{waypoint}", 1).unwrap(), vec![Fragment::Waypoint]);
        assert_eq!(
            parse_fragments("{waypoint|1_1_2}", 1).unwrap(),
            vec![Fragment::WaypointUse { area_id: "1_1_2".into() }]
        );
    }

    #[test]
    fn quest_collects_reward_offer_ids() {
        assert_eq!(
            parse_fragments("{quest|a4q2|a4q2b}", 1).unwrap(),
            vec![Fragment::Quest {
                quest_id: "a4q2".into(),
                reward_offer_ids: vec!["a4q2b".into()],
            }]
        );
        assert_eq!(
            parse_fragments("{quest|a1q1}", 1).unwrap(),
            vec![Fragment::Quest { quest_id: "a1q1".into(), reward_offer_ids: vec![] }]
        );
    }

    #[test]
    fn dir_converts_degrees_to_index() {
        assert_eq!(
            parse_fragments("{dir|270}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 6 }]
        );
        assert_eq!(
            parse_fragments("{dir|45}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 1 }]
        );
        // -45 normalizes to 315 -> index 7
        assert_eq!(
            parse_fragments("{dir|-45}", 1).unwrap(),
            vec![Fragment::Dir { dir_index: 7 }]
        );
        assert!(matches!(
            parse_fragments("{dir|50}", 9),
            Err(FragmentError::InvalidDir { line: 9, .. })
        ));
    }

    #[test]
    fn portal_and_zero_arity_fragments() {
        assert_eq!(parse_fragments("{portal|set}", 1).unwrap(), vec![Fragment::PortalSet]);
        assert_eq!(parse_fragments("{portal|use}", 1).unwrap(), vec![Fragment::PortalUse]);
        assert_eq!(parse_fragments("{logout}", 1).unwrap(), vec![Fragment::Logout]);
        assert_eq!(parse_fragments("{waypoint_get}", 1).unwrap(), vec![Fragment::WaypointGet]);
        assert_eq!(parse_fragments("{trial}", 1).unwrap(), vec![Fragment::Trial]);
    }

    #[test]
    fn reward_vendor_optional_cost() {
        assert_eq!(
            parse_fragments("{reward_vendor|Frostblink|wisdom}", 1).unwrap(),
            vec![Fragment::RewardVendor { item: "Frostblink".into(), cost: Some("wisdom".into()) }]
        );
        assert_eq!(
            parse_fragments("{reward_vendor|Frostblink}", 1).unwrap(),
            vec![Fragment::RewardVendor { item: "Frostblink".into(), cost: None }]
        );
    }

    #[test]
    fn crafting_optional_area() {
        assert_eq!(
            parse_fragments("{crafting}", 1).unwrap(),
            vec![Fragment::Crafting { area_id: None }]
        );
        assert_eq!(
            parse_fragments("{crafting|1_2_5}", 1).unwrap(),
            vec![Fragment::Crafting { area_id: Some("1_2_5".into()) }]
        );
    }

    #[test]
    fn errors_on_unknown_kind_and_unterminated() {
        assert!(matches!(
            parse_fragments("{bogus|x}", 7),
            Err(FragmentError::UnknownKind { line: 7, .. })
        ));
        assert!(matches!(
            parse_fragments("{enter|1_1_2", 8),
            Err(FragmentError::Unterminated { line: 8 })
        ));
        assert!(matches!(
            parse_fragments("{logout|extra}", 2),
            Err(FragmentError::InvalidArity { line: 2, .. })
        ));
    }

    #[test]
    fn comment_only_and_empty_lines_yield_nothing() {
        assert_eq!(parse_fragments("#just a comment", 1).unwrap(), vec![]);
        assert_eq!(parse_fragments("   ", 1).unwrap(), vec![]);
        assert_eq!(parse_fragments("", 1).unwrap(), vec![]);
    }
}
```

Stub above the tests so this compiles:

```rust
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Fragment {
    Text { value: String },
    Kill { name: String },
    Arena { name: String },
    Area { area_id: String },
    Enter { area_id: String },
    Logout,
    Waypoint,
    WaypointUse { area_id: String },
    WaypointGet,
    PortalSet,
    PortalUse,
    Quest { quest_id: String, reward_offer_ids: Vec<String> },
    QuestText { text: String },
    Generic { text: String },
    RewardQuest { item: String },
    RewardVendor { item: String, cost: Option<String> },
    Trial,
    Ascend { version: String },
    Crafting { area_id: Option<String> },
    Dir { dir_index: u8 },
    Copy { text: String },
}

#[derive(Debug, Error, PartialEq)]
pub enum FragmentError {
    #[error("line {line}: unknown fragment kind '{kind}'")]
    UnknownKind { line: usize, kind: String },
    #[error("line {line}: invalid arity for '{kind}'")]
    InvalidArity { line: usize, kind: String },
    #[error("line {line}: unterminated fragment")]
    Unterminated { line: usize },
    #[error("line {line}: invalid dir value '{value}'")]
    InvalidDir { line: usize, value: String },
}

pub fn parse_fragments(_text: &str, _line_no: usize) -> Result<Vec<Fragment>, FragmentError> {
    Ok(vec![])
}
```

`crates/content/src/route_dsl/mod.rs`:

```rust
mod fragment;

pub use fragment::{parse_fragments, Fragment, FragmentError};
```

In `crates/content/src/lib.rs` replace the `crate_ready` placeholder body with:

```rust
//! Content pipeline: route DSL parsing, game data, and content-pack
//! compilation for PoE Campaign Copilot.

pub mod route_dsl;
```

(Delete `crate_ready` and its test module.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p content route_dsl -- --nocapture`
Expected: FAIL — `parses_enter_line_with_arrow_text_and_comment` and others fail (stub returns empty vec).

- [ ] **Step 3: Implement the parser**

Replace the stub `parse_fragments` in `fragment.rs`:

```rust
pub fn parse_fragments(text: &str, line_no: usize) -> Result<Vec<Fragment>, FragmentError> {
    let mut fragments = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if rest.starts_with('#') {
            break; // comment: rest of line ignored
        }
        if let Some(after_brace) = rest.strip_prefix('{') {
            let end = after_brace
                .find('}')
                .ok_or(FragmentError::Unterminated { line: line_no })?;
            let inner = &after_brace[..end];
            fragments.push(parse_one(inner, line_no)?);
            rest = &after_brace[end + 1..];
        } else {
            let stop = rest
                .char_indices()
                .find(|(_, c)| *c == '{' || *c == '#')
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            let (chunk, tail) = rest.split_at(stop);
            if !chunk.trim().is_empty() {
                fragments.push(Fragment::Text { value: chunk.to_string() });
            }
            rest = tail;
        }
    }

    Ok(fragments)
}

fn parse_one(inner: &str, line: usize) -> Result<Fragment, FragmentError> {
    let parts: Vec<&str> = inner.split('|').collect();
    let kind = parts[0];
    let arity = parts.len() - 1;

    let arity_err = || FragmentError::InvalidArity { line, kind: kind.to_string() };

    let frag = match kind {
        "kill" if arity == 1 => Fragment::Kill { name: parts[1].to_string() },
        "arena" if arity == 1 => Fragment::Arena { name: parts[1].to_string() },
        "area" if arity == 1 => Fragment::Area { area_id: parts[1].to_string() },
        "enter" if arity == 1 => Fragment::Enter { area_id: parts[1].to_string() },
        "logout" if arity == 0 => Fragment::Logout,
        "waypoint" if arity == 0 => Fragment::Waypoint,
        "waypoint" if arity == 1 => Fragment::WaypointUse { area_id: parts[1].to_string() },
        "waypoint_get" if arity == 0 => Fragment::WaypointGet,
        "portal" if arity == 1 && parts[1] == "set" => Fragment::PortalSet,
        "portal" if arity == 1 && parts[1] == "use" => Fragment::PortalUse,
        "quest" if arity >= 1 => Fragment::Quest {
            quest_id: parts[1].to_string(),
            reward_offer_ids: parts[2..].iter().map(|s| s.to_string()).collect(),
        },
        "quest_text" if arity == 1 => Fragment::QuestText { text: parts[1].to_string() },
        "generic" if arity == 1 => Fragment::Generic { text: parts[1].to_string() },
        "reward_quest" if arity == 1 => Fragment::RewardQuest { item: parts[1].to_string() },
        "reward_vendor" if (1..=2).contains(&arity) => Fragment::RewardVendor {
            item: parts[1].to_string(),
            cost: parts.get(2).map(|s| s.to_string()),
        },
        "trial" if arity == 0 => Fragment::Trial,
        "ascend" if arity == 1 => Fragment::Ascend { version: parts[1].to_string() },
        "crafting" if arity <= 1 => Fragment::Crafting {
            area_id: parts.get(1).map(|s| s.to_string()),
        },
        "dir" if arity == 1 => {
            let value = parts[1];
            let parsed: f64 = value.parse().map_err(|_| FragmentError::InvalidDir {
                line,
                value: value.to_string(),
            })?;
            let mut deg = parsed % 360.0;
            if deg < 0.0 {
                deg += 360.0;
            }
            if deg % 45.0 != 0.0 {
                return Err(FragmentError::InvalidDir { line, value: value.to_string() });
            }
            Fragment::Dir { dir_index: (deg / 45.0) as u8 }
        }
        "copy" if arity >= 1 => Fragment::Copy { text: parts[1..].concat() },
        "kill" | "arena" | "area" | "enter" | "logout" | "waypoint" | "waypoint_get"
        | "portal" | "quest" | "quest_text" | "generic" | "reward_quest" | "reward_vendor"
        | "trial" | "ascend" | "crafting" | "dir" | "copy" => return Err(arity_err()),
        _ => {
            return Err(FragmentError::UnknownKind { line, kind: kind.to_string() });
        }
    };

    Ok(frag)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p content route_dsl`
Expected: all fragment tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/content/
git commit -m "feat: route DSL fragment model and line parser"
```

---

### Task 5: File-level parser — sections, conditionals, sub-steps

File grammar (from exile-leveling's `route-processing/index.ts`): `#section Name` starts a section; `#ifdef NAME` / `#ifndef NAME` / `#endif` maintain a conditional stack (a line is active only when every stack entry is true); `#sub <fragments>` attaches a hint line to the previous step; any other `#...` line is a comment; anything else is a step line parsed with `parse_fragments`.

**Files:**
- Create: `crates/content/src/route_dsl/parser.rs`
- Modify: `crates/content/src/route_dsl/mod.rs`
- Test: inline in `parser.rs`

**Interfaces:**
- Consumes: `parse_fragments`, `Fragment`, `FragmentError` from Task 4.
- Produces:
  - `pub struct Step { pub line: usize, pub fragments: Vec<Fragment>, pub sub_steps: Vec<Vec<Fragment>> }` (derives `Debug, Clone, PartialEq, Serialize`)
  - `pub struct Section { pub name: String, pub steps: Vec<Step> }` (same derives)
  - `pub enum ParseError` with variants `Fragment(FragmentError)` (`#[from]`), `UnexpectedEndif { line: usize }`, `OrphanSub { line: usize }`, `UnterminatedConditional`
  - `pub fn parse_route_file(source: &str, defines: &BTreeSet<String>) -> Result<Vec<Section>, ParseError>`
  - Re-exported from `route_dsl` as `parse_route_file, Section, Step, ParseError`.

- [ ] **Step 1: Write the failing tests**

`crates/content/src/route_dsl/parser.rs` — test module first (add a stub `parse_route_file` returning `Ok(vec![])`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_dsl::Fragment;
    use std::collections::BTreeSet;

    fn defines(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    const SAMPLE: &str = "\
#section Act 1
Find and kill {kill|Hillock}
➞ {enter|1_1_town} #Lioneye's Watch
#ifdef LEAGUE_START
    Get {waypoint_get}
#endif
#ifndef LEAGUE_START
    {waypoint|1_1_town}
#endif
Find bridge, place {portal|set}
    #sub Go {dir|270}
    #sub Recommended Level: 4
";

    #[test]
    fn splits_sections_and_steps() {
        let sections = parse_route_file(SAMPLE, &defines(&["LEAGUE_START"])).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "Act 1");
        // steps: Hillock, enter town, waypoint_get (ifdef active), portal set
        assert_eq!(sections[0].steps.len(), 4);
        assert_eq!(
            sections[0].steps[2].fragments,
            vec![
                Fragment::Text { value: "Get ".into() },
                Fragment::WaypointGet,
            ]
        );
    }

    #[test]
    fn ifndef_branch_taken_without_define() {
        let sections = parse_route_file(SAMPLE, &defines(&[])).unwrap();
        // steps: Hillock, enter town, waypoint use (ifndef active), portal set
        assert_eq!(sections[0].steps.len(), 4);
        assert_eq!(
            sections[0].steps[2].fragments,
            vec![Fragment::WaypointUse { area_id: "1_1_town".into() }]
        );
    }

    #[test]
    fn subs_attach_to_previous_step() {
        let sections = parse_route_file(SAMPLE, &defines(&["LEAGUE_START"])).unwrap();
        let last = sections[0].steps.last().unwrap();
        assert_eq!(last.sub_steps.len(), 2);
        assert_eq!(
            last.sub_steps[0],
            vec![
                Fragment::Text { value: "Go ".into() },
                Fragment::Dir { dir_index: 6 },
            ]
        );
        assert_eq!(
            last.sub_steps[1],
            vec![Fragment::Text { value: "Recommended Level: 4".into() }]
        );
    }

    #[test]
    fn implicit_default_section_when_no_header() {
        let sections =
            parse_route_file("{logout}\n", &defines(&[])).unwrap();
        assert_eq!(sections[0].name, "Default");
        assert_eq!(sections[0].steps.len(), 1);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            parse_route_file("#endif\n", &defines(&[])),
            Err(ParseError::UnexpectedEndif { line: 1 })
        ));
        assert!(matches!(
            parse_route_file("#section X\n    #sub {dir|90}\n", &defines(&[])),
            Err(ParseError::OrphanSub { line: 2 })
        ));
        assert!(matches!(
            parse_route_file("#ifdef LEAGUE_START\n{logout}\n", &defines(&[])),
            Err(ParseError::UnterminatedConditional)
        ));
    }

    #[test]
    fn inactive_sub_lines_are_skipped() {
        let src = "#section S\n{logout}\n#ifdef X\n    step {generic|inner}\n    #sub hint\n#endif\n";
        let sections = parse_route_file(src, &defines(&[])).unwrap();
        assert_eq!(sections[0].steps.len(), 1); // only logout
        assert!(sections[0].steps[0].sub_steps.is_empty());
    }
}
```

Stub + types above the tests:

```rust
use std::collections::BTreeSet;

use serde::Serialize;
use thiserror::Error;

use super::fragment::{parse_fragments, Fragment, FragmentError};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub line: usize,
    pub fragments: Vec<Fragment>,
    pub sub_steps: Vec<Vec<Fragment>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    pub name: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error(transparent)]
    Fragment(#[from] FragmentError),
    #[error("line {line}: unexpected #endif")]
    UnexpectedEndif { line: usize },
    #[error("line {line}: #sub without a preceding step")]
    OrphanSub { line: usize },
    #[error("unterminated #ifdef/#ifndef block")]
    UnterminatedConditional,
}

pub fn parse_route_file(
    _source: &str,
    _defines: &BTreeSet<String>,
) -> Result<Vec<Section>, ParseError> {
    Ok(vec![])
}
```

Update `crates/content/src/route_dsl/mod.rs`:

```rust
mod fragment;
mod parser;

pub use fragment::{parse_fragments, Fragment, FragmentError};
pub use parser::{parse_route_file, ParseError, Section, Step};
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p content parser`
Expected: FAIL (stub returns empty).

- [ ] **Step 3: Implement**

Replace the stub:

```rust
pub fn parse_route_file(
    source: &str,
    defines: &BTreeSet<String>,
) -> Result<Vec<Section>, ParseError> {
    let mut sections: Vec<Section> = Vec::new();
    let mut cond_stack: Vec<bool> = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        let active = cond_stack.iter().all(|v| *v);

        if let Some(name) = trimmed.strip_prefix("#section") {
            if !cond_stack.is_empty() {
                return Err(ParseError::UnterminatedConditional);
            }
            let name = name.trim();
            let name = if name.is_empty() { "Default" } else { name };
            sections.push(Section { name: name.to_string(), steps: Vec::new() });
            continue;
        }
        if trimmed.starts_with("#endif") {
            if cond_stack.pop().is_none() {
                return Err(ParseError::UnexpectedEndif { line: line_no });
            }
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("#ifdef") {
            cond_stack.push(defines.contains(name.trim()));
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("#ifndef") {
            cond_stack.push(!defines.contains(name.trim()));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#sub") {
            if !active {
                continue;
            }
            let fragments = parse_fragments(rest.trim(), line_no)?;
            if fragments.is_empty() {
                continue;
            }
            let step = sections
                .last_mut()
                .and_then(|s| s.steps.last_mut())
                .ok_or(ParseError::OrphanSub { line: line_no })?;
            step.sub_steps.push(fragments);
            continue;
        }
        if trimmed.starts_with('#') {
            continue; // comment line
        }
        if !active {
            continue;
        }

        let fragments = parse_fragments(trimmed, line_no)?;
        if fragments.is_empty() {
            continue;
        }
        if sections.is_empty() {
            sections.push(Section { name: "Default".to_string(), steps: Vec::new() });
        }
        sections
            .last_mut()
            .expect("section exists")
            .steps
            .push(Step { line: line_no, fragments, sub_steps: Vec::new() });
    }

    if !cond_stack.is_empty() {
        return Err(ParseError::UnterminatedConditional);
    }

    Ok(sections)
}
```

Note: `OrphanSub` must also fire when a `#sub` follows a `#section` with no steps — `sections.last_mut().and_then(...)` handles both that and the no-section case.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p content`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/content/
git commit -m "feat: route file parser with sections, conditionals, sub-steps"
```

---

### Task 6: Integration test — all 10 vendored act files parse cleanly

**Files:**
- Create: `crates/content/tests/parse_vendored_routes.rs`, `crates/content/src/vendor.rs`
- Modify: `crates/content/src/lib.rs`

**Interfaces:**
- Consumes: `parse_route_file` (Task 5), vendored files (Task 3).
- Produces: `content::vendor::vendor_dir() -> std::path::PathBuf` (repo's `vendor/exile-leveling`), `content::vendor::read_act_route(act: u8) -> std::io::Result<String>` — used by Tasks 8–9.

- [ ] **Step 1: Add the vendor path helper**

`crates/content/src/vendor.rs`:

```rust
//! Locations of vendored exile-leveling data (see tools/sync-exile-leveling.sh).

use std::path::PathBuf;

pub fn vendor_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("vendor")
        .join("exile-leveling")
}

pub fn read_act_route(act: u8) -> std::io::Result<String> {
    std::fs::read_to_string(vendor_dir().join("routes").join(format!("act-{act}.txt")))
}
```

Add to `crates/content/src/lib.rs`:

```rust
pub mod vendor;
```

- [ ] **Step 2: Write the integration test**

`crates/content/tests/parse_vendored_routes.rs`:

```rust
use std::collections::BTreeSet;

use content::route_dsl::parse_route_file;
use content::vendor::read_act_route;

fn league_start_defines() -> BTreeSet<String> {
    ["LEAGUE_START".to_string()].into_iter().collect()
}

#[test]
fn all_acts_parse_in_both_variants() {
    for defines in [BTreeSet::new(), league_start_defines()] {
        for act in 1..=10u8 {
            let source = read_act_route(act)
                .unwrap_or_else(|e| panic!("missing vendored route for act {act}: {e}"));
            let sections = parse_route_file(&source, &defines)
                .unwrap_or_else(|e| panic!("act {act} failed to parse: {e}"));
            let step_count: usize = sections.iter().map(|s| s.steps.len()).sum();
            assert!(step_count > 10, "act {act}: suspiciously few steps ({step_count})");
        }
    }
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p content --test parse_vendored_routes`
Expected: PASS. If it FAILS with an `UnknownKind` or arity error, the vendored files use syntax this parser doesn't cover yet — fix the parser (add the missing case per exile-leveling's `fragment/index.ts` semantics), don't relax the test to ignore errors.

- [ ] **Step 4: Commit**

```bash
git add crates/content/
git commit -m "test: parse all vendored act routes in both variants"
```

---

### Task 7: Game data loading (areas, quests, kill-waypoints)

**Files:**
- Create: `crates/content/src/game_data.rs`
- Modify: `crates/content/src/lib.rs`
- Test: inline + assertions against vendored data

**Interfaces:**
- Consumes: `vendor::vendor_dir` (Task 6).
- Produces (all in `content::game_data`):
  - `pub struct Area { pub id: String, pub name: String, pub act: u8, pub has_waypoint: bool, pub is_town_area: bool, pub parent_town_area_id: Option<String>, pub connection_ids: Vec<String>, pub crafting_recipes: Vec<String> }` (derives `Debug, Clone, serde::Deserialize`)
  - `pub struct Quest { pub id: String, pub name: String }` (same derives; unknown JSON fields ignored)
  - `pub type AreaMap = std::collections::BTreeMap<String, Area>;`
  - `pub type QuestMap = std::collections::BTreeMap<String, Quest>;`
  - `pub fn load_areas(json: &str) -> Result<AreaMap, serde_json::Error>`
  - `pub fn load_quests(json: &str) -> Result<QuestMap, serde_json::Error>`
  - `pub fn load_vendored() -> anyhow-free Result<(AreaMap, QuestMap), GameDataError>` — reads both files from `vendor_dir()`; `GameDataError` is a thiserror enum with `Io(#[from] std::io::Error)` and `Json(#[from] serde_json::Error)`.

- [ ] **Step 1: Write the failing test**

Test module at the bottom of the new `crates/content/src/game_data.rs` (create the file with the struct definitions from the Interfaces block above and stub loaders that call `serde_json::from_str`— the real work is getting the shapes right, so the "failing" state here is a compile failure until the file exists):

```rust
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

        assert!(areas.len() > 100);
        assert!(quests.len() > 40);
    }
}
```

- [ ] **Step 2: Implement**

Full `crates/content/src/game_data.rs` above the tests:

```rust
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
}

pub type AreaMap = BTreeMap<String, Area>;
pub type QuestMap = BTreeMap<String, Quest>;

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

pub fn load_vendored() -> Result<(AreaMap, QuestMap), GameDataError> {
    let data = vendor_dir().join("data");
    let areas = load_areas(&std::fs::read_to_string(data.join("areas.json"))?)?;
    let quests = load_quests(&std::fs::read_to_string(data.join("quests.json"))?)?;
    Ok((areas, quests))
}
```

Add to `crates/content/src/lib.rs`:

```rust
pub mod game_data;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p content game_data`
Expected: PASS. If deserialization fails on a field (e.g., an unexpected null), adjust that field's type to `Option<...>` to match the vendored JSON — the vendored file is the source of truth.

- [ ] **Step 4: Commit**

```bash
git add crates/content/
git commit -m "feat: load vendored areas and quests game data"
```

---

### Task 8: Stateful route walk — area context per step

The walk replays exile-leveling's evaluator semantics to compute, for every step, which area the player is in when the step is shown ("area context"), and validates all area/quest references. Transition rules (from `fragment/index.ts`):

- `enter|X` / `waypoint|X`: current becomes X; entering a town sets `last_town`.
- `logout`: current becomes `last_town`; portal cleared.
- `portal|set`: portal = current (error if current is a town).
- `portal|use`: if portal is unset and current is not a town, portal implicitly becomes current. Then: if current == portal area → move to that area's parent town; else if current is the portal area's parent town → move to portal area and clear portal; else error.
- `ascend|_`: current becomes `last_town`.
- All other fragments don't move the player.

**Files:**
- Create: `crates/content/src/walk.rs`
- Modify: `crates/content/src/lib.rs`
- Test: inline golden test against vendored act 1

**Interfaces:**
- Consumes: `Section`, `Step`, `Fragment` (Tasks 4–5); `AreaMap`, `QuestMap`, `load_vendored` (Task 7); `read_act_route` (Task 6).
- Produces (in `content::walk`):
  - `pub struct WalkState { pub current_area_id: String, pub last_town_area_id: String, pub portal_area_id: Option<String> }` with `impl WalkState { pub fn campaign_start() -> Self }` (current `1_1_1`, town `1_1_town`, portal `None`)
  - `pub struct CompiledStep { pub id: String, pub act: u8, pub section: String, pub area_context: String, pub fragments: Vec<Fragment>, pub sub_steps: Vec<Vec<Fragment>> }` (derives `Debug, Clone, PartialEq, Serialize`)
  - `pub enum WalkError` (thiserror): `UnknownArea { step_id: String, area_id: String }`, `UnknownQuest { step_id: String, quest_id: String }`, `PortalMisuse { step_id: String, detail: String }`
  - `pub fn walk_act(act: u8, sections: &[Section], areas: &AreaMap, quests: &QuestMap, state: &mut WalkState) -> Result<Vec<CompiledStep>, WalkError>` — step ids formatted `a{act}-s{index:03}` (index across the whole act, 1-based).

- [ ] **Step 1: Write the failing test**

Test module at the bottom of the new `crates/content/src/walk.rs` (create the file with types + a stub `walk_act` returning `Ok(vec![])`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::load_vendored;
    use crate::route_dsl::parse_route_file;
    use crate::vendor::read_act_route;
    use std::collections::BTreeSet;

    #[test]
    fn act1_league_start_golden_contexts() {
        let (areas, quests) = load_vendored().unwrap();
        let defines: BTreeSet<String> = ["LEAGUE_START".to_string()].into_iter().collect();
        let sections = parse_route_file(&read_act_route(1).unwrap(), &defines).unwrap();

        let mut state = WalkState::campaign_start();
        let steps = walk_act(1, &sections, &areas, &quests, &mut state).unwrap();

        let contexts: Vec<&str> =
            steps.iter().take(12).map(|s| s.area_context.as_str()).collect();
        assert_eq!(
            contexts,
            vec![
                "1_1_1",     // kill Hillock (Twilight Strand)
                "1_1_1",     // enter Lioneye's Watch
                "1_1_town",  // hand in Enemy at the Gate
                "1_1_town",  // enter The Coast
                "1_1_2",     // get waypoint (league start)
                "1_1_2",     // enter The Mud Flats
                "1_1_3",     // find 3x Glyph
                "1_1_3",     // enter The Submerged Passage
                "1_1_4_1",   // waypoint to The Coast
                "1_1_2",     // enter The Tidal Island
                "1_1_2a",    // kill Hailrake
                "1_1_2a",    // logout
            ]
        );

        assert_eq!(steps[0].id, "a1-s001");
        assert_eq!(steps[0].act, 1);
    }

    #[test]
    fn walk_validates_area_ids() {
        let (areas, quests) = load_vendored().unwrap();
        let sections = parse_route_file(
            "➞ {enter|not_a_real_area}\n",
            &BTreeSet::new(),
        )
        .unwrap();
        let mut state = WalkState::campaign_start();
        let err = walk_act(1, &sections, &areas, &quests, &mut state).unwrap_err();
        assert!(matches!(err, WalkError::UnknownArea { .. }));
    }

    #[test]
    fn full_campaign_walks_without_errors_in_both_variants() {
        let (areas, quests) = load_vendored().unwrap();
        for defines in [
            BTreeSet::new(),
            ["LEAGUE_START".to_string()].into_iter().collect::<BTreeSet<_>>(),
        ] {
            let mut state = WalkState::campaign_start();
            for act in 1..=10u8 {
                let sections =
                    parse_route_file(&read_act_route(act).unwrap(), &defines).unwrap();
                walk_act(act, &sections, &areas, &quests, &mut state)
                    .unwrap_or_else(|e| panic!("act {act} walk failed: {e}"));
            }
        }
    }
}
```

Types + stub above the tests:

```rust
//! Stateful walk over parsed route sections: resolves the player's area
//! context for every step and validates area/quest references.

use serde::Serialize;
use thiserror::Error;

use crate::game_data::{AreaMap, QuestMap};
use crate::route_dsl::{Fragment, Section};

#[derive(Debug, Clone)]
pub struct WalkState {
    pub current_area_id: String,
    pub last_town_area_id: String,
    pub portal_area_id: Option<String>,
}

impl WalkState {
    pub fn campaign_start() -> Self {
        Self {
            current_area_id: "1_1_1".to_string(),
            last_town_area_id: "1_1_town".to_string(),
            portal_area_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompiledStep {
    pub id: String,
    pub act: u8,
    pub section: String,
    pub area_context: String,
    pub fragments: Vec<Fragment>,
    pub sub_steps: Vec<Vec<Fragment>>,
}

#[derive(Debug, Error)]
pub enum WalkError {
    #[error("step {step_id}: unknown area id '{area_id}'")]
    UnknownArea { step_id: String, area_id: String },
    #[error("step {step_id}: unknown quest id '{quest_id}'")]
    UnknownQuest { step_id: String, quest_id: String },
    #[error("step {step_id}: portal misuse: {detail}")]
    PortalMisuse { step_id: String, detail: String },
}

pub fn walk_act(
    _act: u8,
    _sections: &[Section],
    _areas: &AreaMap,
    _quests: &QuestMap,
    _state: &mut WalkState,
) -> Result<Vec<CompiledStep>, WalkError> {
    Ok(vec![])
}
```

Add to `crates/content/src/lib.rs`:

```rust
pub mod walk;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p content walk`
Expected: FAIL — golden test gets empty steps.

- [ ] **Step 3: Implement**

Replace the stub:

```rust
pub fn walk_act(
    act: u8,
    sections: &[Section],
    areas: &AreaMap,
    quests: &QuestMap,
    state: &mut WalkState,
) -> Result<Vec<CompiledStep>, WalkError> {
    let mut compiled = Vec::new();
    let mut index = 0usize;

    for section in sections {
        for step in &section.steps {
            index += 1;
            let step_id = format!("a{act}-s{index:03}");
            let area_context = state.current_area_id.clone();

            for fragment in &step.fragments {
                apply_fragment(fragment, areas, quests, state, &step_id)?;
            }
            for sub in &step.sub_steps {
                for fragment in sub {
                    apply_fragment(fragment, areas, quests, state, &step_id)?;
                }
            }

            compiled.push(CompiledStep {
                id: step_id,
                act,
                section: section.name.clone(),
                area_context,
                fragments: step.fragments.clone(),
                sub_steps: step.sub_steps.clone(),
            });
        }
    }

    Ok(compiled)
}

fn transition(state: &mut WalkState, areas: &AreaMap, area_id: &str) {
    if let Some(area) = areas.get(area_id) {
        if area.is_town_area {
            state.last_town_area_id = area.id.clone();
        }
    }
    state.current_area_id = area_id.to_string();
}

fn require_area<'a>(
    areas: &'a AreaMap,
    area_id: &str,
    step_id: &str,
) -> Result<&'a crate::game_data::Area, WalkError> {
    areas.get(area_id).ok_or_else(|| WalkError::UnknownArea {
        step_id: step_id.to_string(),
        area_id: area_id.to_string(),
    })
}

fn apply_fragment(
    fragment: &Fragment,
    areas: &AreaMap,
    quests: &QuestMap,
    state: &mut WalkState,
    step_id: &str,
) -> Result<(), WalkError> {
    match fragment {
        Fragment::Enter { area_id } | Fragment::WaypointUse { area_id } => {
            require_area(areas, area_id, step_id)?;
            transition(state, areas, area_id);
        }
        Fragment::Area { area_id } | Fragment::Crafting { area_id: Some(area_id) } => {
            require_area(areas, area_id, step_id)?;
        }
        Fragment::Logout | Fragment::Ascend { .. } => {
            let town = state.last_town_area_id.clone();
            transition(state, areas, &town);
            if matches!(fragment, Fragment::Logout) {
                state.portal_area_id = None;
            }
        }
        Fragment::PortalSet => {
            let current = require_area(areas, &state.current_area_id.clone(), step_id)?;
            if current.is_town_area {
                return Err(WalkError::PortalMisuse {
                    step_id: step_id.to_string(),
                    detail: "portal cannot be set in town".to_string(),
                });
            }
            state.portal_area_id = Some(state.current_area_id.clone());
        }
        Fragment::PortalUse => {
            let current_id = state.current_area_id.clone();
            let current = require_area(areas, &current_id, step_id)?;
            if state.portal_area_id.as_deref() != Some(current_id.as_str())
                && !current.is_town_area
            {
                state.portal_area_id = Some(current_id.clone());
            }
            let portal_id = state.portal_area_id.clone().ok_or_else(|| {
                WalkError::PortalMisuse {
                    step_id: step_id.to_string(),
                    detail: "portal not set".to_string(),
                }
            })?;
            let portal_area = require_area(areas, &portal_id, step_id)?;

            if current_id == portal_id {
                let town = portal_area.parent_town_area_id.clone().ok_or_else(|| {
                    WalkError::PortalMisuse {
                        step_id: step_id.to_string(),
                        detail: format!("area {portal_id} has no parent town"),
                    }
                })?;
                transition(state, areas, &town);
            } else if portal_area.parent_town_area_id.as_deref() == Some(current_id.as_str()) {
                transition(state, areas, &portal_id);
                state.portal_area_id = None;
            } else {
                return Err(WalkError::PortalMisuse {
                    step_id: step_id.to_string(),
                    detail: "portal used from unrelated area".to_string(),
                });
            }
        }
        Fragment::Quest { quest_id, .. } => {
            if !quests.contains_key(quest_id) {
                return Err(WalkError::UnknownQuest {
                    step_id: step_id.to_string(),
                    quest_id: quest_id.clone(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p content`
Expected: all PASS, including the full-campaign walk in both variants. If `full_campaign_walks_without_errors_in_both_variants` fails on a specific act, print the failing step id from the error, open the vendored act file at that step, and adjust the walk semantics to match exile-leveling's evaluator (the rules at the top of this task) — do not special-case step ids.

- [ ] **Step 5: Commit**

```bash
git add crates/content/
git commit -m "feat: stateful route walk with area contexts and validation"
```

---

### Task 9: Content-pack compiler binary

**Files:**
- Create: `crates/content/src/compile.rs`, `crates/content/src/bin/compile-content.rs`
- Modify: `crates/content/src/lib.rs`
- Test: inline in `compile.rs`

**Interfaces:**
- Consumes: everything from Tasks 4–8.
- Produces (in `content::compile`):
  - `pub enum Variant { LeagueStart, Standard }` with `impl Variant { pub fn defines(&self) -> BTreeSet<String>; pub fn file_stem(&self) -> &'static str }` (`"standard-fast.league-start"` / `"standard-fast.standard"`)
  - `pub struct SourceInfo { pub project: String, pub reference: String, pub license: String }` (Serialize)
  - `pub struct ActRoute { pub act: u8, pub steps: Vec<CompiledStep> }` (Serialize)
  - `pub struct ContentPack { pub format_version: u32, pub source: SourceInfo, pub variant: String, pub acts: Vec<ActRoute> }` (Serialize; `format_version` = 1)
  - `pub enum CompileError` (thiserror): `Io(#[from] std::io::Error)`, `Parse(#[from] ParseError)`, `Walk(#[from] WalkError)`, `GameData(#[from] GameDataError)`
  - `pub fn compile_route_pack(variant: Variant) -> Result<ContentPack, CompileError>` — parses and walks acts 1–10 from vendored data.
  - Binary `compile-content`: writes both variants as pretty JSON to `content-pack/routes/<stem>.json`.

- [ ] **Step 1: Write the failing test**

Test module at the bottom of the new `crates/content/src/compile.rs` (create with types + stub `compile_route_pack` returning an empty pack):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_both_variants_across_all_acts() {
        for variant in [Variant::LeagueStart, Variant::Standard] {
            let pack = compile_route_pack(variant).expect("pack compiles");
            assert_eq!(pack.format_version, 1);
            assert_eq!(pack.acts.len(), 10);
            for act_route in &pack.acts {
                assert!(
                    !act_route.steps.is_empty(),
                    "act {} has no steps",
                    act_route.act
                );
            }
            // serializes without error and includes tagged fragments
            let json = serde_json::to_string(&pack).unwrap();
            assert!(json.contains("\"type\":\"enter\""));
            assert!(json.contains("\"area_context\""));
        }
    }
}
```

- [ ] **Step 2: Implement**

Full `crates/content/src/compile.rs` above the tests:

```rust
//! Compiles vendored route data into versioned content-pack JSON.

use std::collections::BTreeSet;

use serde::Serialize;
use thiserror::Error;

use crate::game_data::{load_vendored, GameDataError};
use crate::route_dsl::{parse_route_file, ParseError};
use crate::vendor::read_act_route;
use crate::walk::{walk_act, CompiledStep, WalkError, WalkState};

pub const EXILE_LEVELING_REF: &str = "1961ef839235e831ee61413de331cce42eb78f24";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Variant {
    LeagueStart,
    Standard,
}

impl Variant {
    pub fn defines(&self) -> BTreeSet<String> {
        match self {
            Variant::LeagueStart => ["LEAGUE_START".to_string()].into_iter().collect(),
            Variant::Standard => BTreeSet::new(),
        }
    }

    pub fn file_stem(&self) -> &'static str {
        match self {
            Variant::LeagueStart => "standard-fast.league-start",
            Variant::Standard => "standard-fast.standard",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Variant::LeagueStart => "league_start",
            Variant::Standard => "standard",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub project: String,
    pub reference: String,
    pub license: String,
}

#[derive(Debug, Serialize)]
pub struct ActRoute {
    pub act: u8,
    pub steps: Vec<CompiledStep>,
}

#[derive(Debug, Serialize)]
pub struct ContentPack {
    pub format_version: u32,
    pub source: SourceInfo,
    pub variant: String,
    pub acts: Vec<ActRoute>,
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("route parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("route walk error: {0}")]
    Walk(#[from] WalkError),
    #[error("game data error: {0}")]
    GameData(#[from] GameDataError),
}

pub fn compile_route_pack(variant: Variant) -> Result<ContentPack, CompileError> {
    let (areas, quests) = load_vendored()?;
    let defines = variant.defines();
    let mut state = WalkState::campaign_start();
    let mut acts = Vec::with_capacity(10);

    for act in 1..=10u8 {
        let sections = parse_route_file(&read_act_route(act)?, &defines)?;
        let steps = walk_act(act, &sections, &areas, &quests, &mut state)?;
        acts.push(ActRoute { act, steps });
    }

    Ok(ContentPack {
        format_version: 1,
        source: SourceInfo {
            project: "HeartofPhos/exile-leveling".to_string(),
            reference: EXILE_LEVELING_REF.to_string(),
            license: "MIT".to_string(),
        },
        variant: variant.label().to_string(),
        acts,
    })
}
```

`crates/content/src/bin/compile-content.rs`:

```rust
//! Writes compiled route content packs to content-pack/routes/.

use std::path::PathBuf;

use content::compile::{compile_route_pack, Variant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("content-pack")
        .join("routes");
    std::fs::create_dir_all(&out_dir)?;

    for variant in [Variant::LeagueStart, Variant::Standard] {
        let pack = compile_route_pack(variant)?;
        let path = out_dir.join(format!("{}.json", variant.file_stem()));
        std::fs::write(&path, serde_json::to_string_pretty(&pack)?)?;
        let steps: usize = pack.acts.iter().map(|a| a.steps.len()).sum();
        println!("wrote {} ({} steps)", path.display(), steps);
    }

    Ok(())
}
```

Add to `crates/content/src/lib.rs`:

```rust
pub mod compile;
```

- [ ] **Step 3: Run tests and the binary**

Run: `cargo test -p content compile`
Expected: PASS.

Run: `cargo run -p content --bin compile-content`
Expected: two `wrote .../content-pack/routes/standard-fast.*.json (<N> steps)` lines, N in the hundreds. `content-pack/` is gitignored (Task 1) — it is generated output.

- [ ] **Step 4: Commit**

```bash
git add crates/content/
git commit -m "feat: content-pack compiler for both route variants"
```

---

### Task 10: Tauri 2 app scaffold

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/capabilities/default.json`, `src-tauri/icons/*`
- Modify: `Cargo.toml` (workspace members), `.gitignore`, `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: workspace from Task 1.
- Produces: `poe-copilot-app` binary crate at `src-tauri/`, frontend at repo root `src/`. Later plans add overlay/settings windows and IPC commands here.

- [ ] **Step 1: Write frontend files**

`package.json`:

```json
{
  "name": "poe-campaign-copilot-ui",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "react": "^18",
    "react-dom": "^18"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "@types/react": "^18",
    "@types/react-dom": "^18",
    "@vitejs/plugin-react": "^4",
    "typescript": "^5",
    "vite": "^6"
  }
}
```

`vite.config.ts`:

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
```

`tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "isolatedModules": true
  },
  "include": ["src"]
}
```

`index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>PoE Campaign Copilot</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`src/App.tsx`:

```tsx
export default function App() {
  return (
    <main style={{ fontFamily: "system-ui", padding: "2rem" }}>
      <h1>PoE Campaign Copilot</h1>
      <p>Development shell. Overlay and settings UI land in later plans.</p>
    </main>
  );
}
```

- [ ] **Step 2: Write the Tauri crate**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "poe-copilot-app"
version = "0.1.0"
edition = "2024"
license = "MIT"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
content = { path = "../crates/content" }
```

`src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "PoE Campaign Copilot",
  "version": "0.1.0",
  "identifier": "com.andrewli.poe-campaign-copilot",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "PoE Campaign Copilot",
        "width": 900,
        "height": 620
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": false,
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```

`src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

Update root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/content", "src-tauri"]
```

Append to `.gitignore`:

```
node_modules/
dist/
src-tauri/gen/
```

- [ ] **Step 3: Generate placeholder icons**

Tauri's context macro needs icon files. Create a placeholder source image and let the Tauri CLI generate the set:

```bash
# 1x1 dark-gold PNG, upscaled to 1024 for icon generation
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNsb2j4DwAFKAJ003oL8QAAAABJRU5ErkJggg==' | base64 -d > /tmp/pixel.png
sips -z 1024 1024 /tmp/pixel.png --out /tmp/app-icon.png
npm install
npx tauri icon /tmp/app-icon.png
```

Expected: `src-tauri/icons/` populated (32x32.png, 128x128.png, icon.icns, icon.ico, etc.).

- [ ] **Step 4: Verify build on macOS**

Run: `npm run build`
Expected: tsc + vite complete, `dist/` created.

Run: `cargo check -p poe-copilot-app`
Expected: compiles with no errors.

Run: `npm run tauri dev` — wait for a desktop window titled "PoE Campaign Copilot" showing the placeholder page, then quit it (Ctrl+C in terminal).
Expected: window opens on macOS. This is the manual checkpoint for this task.

- [ ] **Step 5: Add frontend + app job to CI**

Append to `.github/workflows/ci.yml` (same indentation level as the `rust:` job):

```yaml
  app:
    strategy:
      fail-fast: false
      matrix:
        os: [macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: npm ci
      - run: npm run build
      - run: cargo check -p poe-copilot-app
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: Tauri 2 app scaffold with React frontend and CI job"
```

---

## Verification (end of plan)

- [ ] `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — all green.
- [ ] `cargo run -p content --bin compile-content` — writes both variant packs, hundreds of steps each.
- [ ] `npm run tauri dev` — desktop window opens on macOS.
- [ ] `git log --oneline` shows one commit per task, conventional format.

## Self-Review Notes

- Spec coverage (this plan's slice): repo/CI/licensing (§6) — Tasks 1–2; route data vendoring + DSL parse + compile (§3) — Tasks 3–9; Tauri app base (§2) — Task 10. Deliberately out of scope here (later plans): docx extraction (§4), overlay UX (§5), log tailing/replay (§7), PoB, engines.
- Type consistency: `Fragment`/`Section`/`Step` (Tasks 4–5) are consumed by name in Tasks 6, 8, 9; `CompiledStep` (Task 8) is serialized by Task 9; `vendor_dir`/`read_act_route` (Task 6) used in 7–9. Step id format `a{act}-s{index:03}` defined once (Task 8) and referenced in Task 9's output only through `CompiledStep`.
- Known risk: vendored files may contain fragment syntax not covered by the sample studied (act 1 + evaluator source). Task 6 and Task 8 Step 4 include explicit fix-forward instructions (extend parser/walk to match exile-leveling semantics; never relax tests).
