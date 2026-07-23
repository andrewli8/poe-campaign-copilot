# Plan 7: Windows Installer

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A downloadable Windows setup `.exe` (NSIS, WebView2 bootstrapper included) built by GitHub Actions, so a user installs and runs the overlay without Rust, Node, or a repo checkout.

**Architecture:** The one real blocker is data paths: `content::vendor_dir()`/`layouts_dir()` resolve via `CARGO_MANIFEST_DIR`, which doesn't exist for an installed binary. Fix with a process-wide data root: a `OnceLock<PathBuf>` in the `content` crate, defaulting to the repo root (today's behavior, keeps every test green), settable exactly once by the app at startup to Tauri's resource directory when a bundled data layout is detected. Bundling maps `vendor/` and `content/layouts/` into resources with clean names (no `_up_` paths). A `release.yml` workflow builds the NSIS installer on `windows-latest` (manual dispatch + version tags) and uploads it as an artifact / GitHub release asset.

**Tech Stack:** Tauri 2 NSIS bundler, GitHub Actions, existing crates.

## Global Constraints

- Default behavior unchanged: with no explicit data root set, everything resolves exactly as today (all existing tests pass untouched).
- `set_data_root` is set-once (`OnceLock`); calling it after any path lookup has occurred must not relocate data mid-run (document: the app calls it before constructing the Pipeline).
- Resource mapping must produce `resource_dir/vendor/exile-leveling/...` and `resource_dir/content/layouts/...` (use tauri.conf's map-form `resources` to avoid `_up_` parent-dir mangling).
- The app auto-detects: if `resource_dir/vendor/exile-leveling` exists, use the resource root; else stay on the dev default. `POE_COPILOT_DATA_ROOT` env var overrides both (diagnostics/tests).
- Bundle identifier/product name unchanged; `bundle.targets` = `["nsis"]`; `bundle.active` = true only where it doesn't break `tauri dev`/mac workflows (bundle step runs in the Windows CI job; macOS local `tauri build` is not a supported path and CI's existing `cargo check` app job must stay green).
- NSIS `installMode`: `currentUser` (no admin prompt). WebView2 bootstrapper: default (downloads if missing).
- Release workflow: `workflow_dispatch` + `push: tags: v*`; artifact always uploaded; a GitHub Release with the exe attached on tag builds only.
- Version stays 0.1.0 until first tag; first tag will be `v0.1.0`.
- Edition 2024; commit format; no AI attribution. Controller pushes after each task.

---

### Task 1: Data-root resolution in `content`

**Files:**
- Modify: `crates/content/src/vendor.rs`, `crates/content/src/layouts.rs`, `crates/content/src/lib.rs`

**Interfaces:**
- New module `content::data_root`:
  - `pub fn set_data_root(root: PathBuf) -> Result<(), PathBuf>` — sets the `OnceLock`; `Err` returns the rejected value if already set.
  - `pub fn data_root() -> PathBuf` — precedence: `POE_COPILOT_DATA_ROOT` env var (read each call) → the `OnceLock` value → `CARGO_MANIFEST_DIR/../..` (repo root, compile-time default).
- `vendor::vendor_dir()` becomes `data_root().join("vendor").join("exile-leveling")`; `layouts::layouts_dir()` becomes `data_root().join("content").join("layouts")`. Everything downstream (gems, areas, assets, compile) inherits automatically — verify by grep that no other `CARGO_MANIFEST_DIR` data path remains in non-test library code (`replay::fixtures_dir` may stay dev-only; the compile-content bin's `content-pack` OUTPUT path may stay repo-relative — it's a dev tool).

- [ ] **Step 1 (failing test):** in a new `#[cfg(test)]` module for `data_root`: with env var set to a temp path, `data_root()` returns it; without env, returns the repo root (assert `vendor_dir().join("routes/act-1.txt")` exists — proves the default still works). Use a serial-safe pattern for the env test (set/remove within one test; Rust tests run threaded — use a unique var read... simplest: make precedence testable via an internal `fn resolve(env: Option<PathBuf>, once: Option<&PathBuf>) -> PathBuf` pure function and test THAT for precedence; the public `data_root()` just feeds it. Avoids env races entirely; one integration-ish assert on the default path only.)
- [ ] **Step 2:** RED → implement → GREEN; full workspace tests green (they all ride the default path); fmt/clippy clean.
- [ ] **Step 3:** Commit: `feat: overridable data root for packaged builds`

---

### Task 2: Bundling config + app resource detection

**Files:**
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml` (if a bundle feature flag is needed)

**Interfaces:**
- `tauri.conf.json` `bundle`:
  ```json
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.icns", "icons/icon.ico"],
    "resources": { "../vendor": "vendor", "../content/layouts": "content/layouts" },
    "windows": { "nsis": { "installMode": "currentUser" } }
  }
  ```
  (Reconcile exact NSIS option key names against the installed Tauri schema; record final shape.)
- `main.rs`, at the very top of `.setup()` BEFORE `Pipeline::new` is called (note: `Pipeline::new` currently runs in `main()` before the builder — move its construction into `.setup()` or resolve the resource dir earlier via `tauri::Builder`... cleanest: move pipeline construction into `.setup()` after the data-root detection, keeping the `AppState` managed with a `Mutex<Option<Pipeline>>` OR construct `AppState` with the pipeline built inside `.setup()` via `app.manage(...)`. Choose the minimal restructuring that keeps all commands working; document the choice):
  ```rust
  if let Ok(resource_dir) = app.path().resource_dir() {
      if resource_dir.join("vendor").join("exile-leveling").is_dir() {
          if let Err(rejected) = content::data_root::set_data_root(resource_dir.clone()) {
              eprintln!("data root already set; ignoring {rejected:?}");
          }
      }
  }
  ```
- Dev behavior unchanged: `tauri dev`'s resource dir has no `vendor/`, detection falls through to the repo default.

- [ ] **Step 1:** Implement config + detection + any pipeline-construction move. `cargo test --workspace` green; `cargo check -p poe-copilot-app` clean; `npm run build` clean; brief `npm run tauri dev` smoke on macOS: launches, waiting state, no regressions (bundle.active=true must not break dev).
- [ ] **Step 2:** Simulated-bundle test of the detection logic where feasible: stage a temp dir with `vendor/exile-leveling` copied (or symlinked) + `content/layouts`, set `POE_COPILOT_DATA_ROOT` to it, run `cargo test -p content` data-root default test exclusions aside — at minimum, run the app's pipeline unit tests with the env var pointing at the real repo root explicitly (proves env-var path end-to-end).
- [ ] **Step 3:** Commit: `feat: bundle data resources and detect packaged data root`

---

### Task 3: Release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Triggers: `workflow_dispatch` (always available) and `push: tags: ["v*"]`.
- One job, `windows-latest`: checkout, setup-node 22 + npm cache, dtolnay stable Rust, Swatinem cache, `npm ci`, `npm run tauri build` (produces `src-tauri/target/release/bundle/nsis/*-setup.exe`), `actions/upload-artifact` with the exe (name `poe-campaign-copilot-windows-setup`), and on tag refs additionally `softprops/action-gh-release` (or `gh release create`) attaching the exe.
- `permissions: contents: write` (release creation).

- [ ] **Step 1:** Write the workflow; validate YAML (`gh workflow view` after push or a YAML lint).
- [ ] **Step 2:** Commit: `ci: windows installer release workflow`. (Controller pushes and dispatches the first run — the run itself is the real test; be prepared for one iteration if the NSIS step surfaces config issues. If the dispatched run fails, read the log, fix, commit again — the task is done when a dispatched run produces a downloadable setup exe artifact.)

---

### Task 4: README download instructions

**Files:**
- Modify: `README.md`

- [ ] **Step 1:** Add an "Install on Windows (easiest)" subsection ABOVE the from-source quick start: download the latest `poe-campaign-copilot-windows-setup` artifact from the Actions "Release" workflow (link) or the latest GitHub Release once tags exist; run the setup exe (per-user install, no admin); first-launch steps are the same Settings flow as below. Keep the humanized style: no em dashes, plain sentences, honest note that SmartScreen will warn on an unsigned installer and how to proceed (More info → Run anyway) — unsigned is the current reality, code signing is future work.
- [ ] **Step 2:** Full gate; commit: `docs: installer download instructions`

---

## Verification (end of plan)

- [ ] Workspace + vitest gates green; mac `tauri dev` unaffected.
- [ ] Dispatched release workflow run produces a downloadable Windows setup exe artifact.
- [ ] README explains the download path first, source path second.

## Self-Review Notes

- The riskiest unknown is the first `tauri build` on the Windows runner (NSIS options, resource globs, icon set). The plan deliberately makes Task 3's exit condition "a dispatched run produced the exe", not "the YAML exists".
- Not in scope: code signing (SmartScreen warning documented instead), auto-updates, macOS bundles, and the audit-phase items. `replay::fixtures_dir` and compile-content output stay dev-only by design.
