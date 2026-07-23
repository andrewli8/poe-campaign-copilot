# Plan 8: Auto-Update Notifier

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the Settings window opens, check GitHub Releases for a newer signed version; if one exists, show a banner in Settings with an "Update and restart" button that downloads, installs, and relaunches. No network during play.

**Architecture:** Tauri 2 `updater` plugin. The app embeds the updater public key and a GitHub Releases endpoint. On Settings-window mount, the frontend calls the plugin's `check()` (a Rust-side network call to the releases `latest.json` manifest — not a webview fetch, so the strict CSP is unaffected). If an update is available, `SettingsPage` renders an update banner; the button calls `downloadAndInstall()` then `relaunch()` (process plugin). The release workflow signs the NSIS bundle with the private key (GitHub secret, already set) and publishes a `latest.json` manifest alongside the installer.

**Tech Stack:** tauri-plugin-updater 2, tauri-plugin-process 2, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`, existing React/vitest.

## Global Constraints

- **Passive-only preserved:** the update check runs ONLY when the Settings window opens (never on launch, never during play). The overlay pipeline/tailer are untouched. Document this at the check call site.
- **Network scope:** the updater's only network destinations are GitHub Releases (`github.com` / `objects.githubusercontent.com`). No other host. The webview CSP stays `default-src 'self'` (the check is Rust-plugin IPC, not a webview `fetch`), so CSP is NOT loosened.
- **Signing is mandatory:** updates are ed25519-signed. Public key (embed in config, safe to commit): `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDZCQTE1NERGNkIxMTg3MUYKUldRZmh4RnIzMVNoYThzZXNHbjk2aDFiY2plWnlhRnd5cUEwbEhidVJya1RabVlmRlVWRHV4WDgK`. Private key is the GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY` (already set; password secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is empty).
- **Updater bootstrap reality (document in README):** the currently-installed v0.1.1 has no updater, so it cannot auto-update. The first updater-enabled build (v0.1.2, tagged at end of this plan) must be installed manually once; every release after that auto-updates from it.
- **Endpoint:** `https://github.com/andrewli8/poe-campaign-copilot/releases/latest/download/latest.json`.
- Edition 2024; commit format; no AI attribution. Controller pushes after each task; controller cuts the final tag.

---

### Task 1: Updater + process plugin wiring

**Files:**
- Modify: `src-tauri/Cargo.toml` (+ `tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`), `src-tauri/src/main.rs` (register both plugins), `src-tauri/tauri.conf.json` (updater plugin config + `bundle.createUpdaterArtifacts`), `src-tauri/capabilities/default.json`, `package.json` (+ `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`)

**Interfaces / config:**
- `tauri.conf.json`:
  - `bundle.createUpdaterArtifacts: true` (v2 flag that makes `tauri build` emit the `.sig` + updater-compatible artifacts).
  - `plugins.updater`: `{ "endpoints": ["https://github.com/andrewli8/poe-campaign-copilot/releases/latest/download/latest.json"], "pubkey": "<the pubkey above>" }`. Reconcile exact key names (`pubkey` vs `publicKey`) against the installed plugin's config schema; record final shape.
- `main.rs`: `.plugin(tauri_plugin_updater::Builder::new().build())` and `.plugin(tauri_plugin_process::init())`.
- `capabilities/default.json`: add `updater:default` and `process:allow-restart` (or `core:allow-restart` — reconcile the exact permission id that enables `relaunch()`; record it).

- [ ] **Step 1:** Apply Cargo.toml + package.json deps (`npm install`), register plugins in main.rs, add config + capabilities. Reconcile all schema/permission ids against installed versions; record every final value in the report.
- [ ] **Step 2:** `cargo check -p poe-copilot-app`, `cargo test --workspace`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `npm run build`, `npm test` — all green. Brief `npm run tauri dev` smoke on macOS: launches clean (updater plugin registers without panic; no check fires yet since that's Task 3).
- [ ] **Step 3:** Commit: `feat: register updater and process plugins with signing config`

---

### Task 2: Release workflow signs builds and publishes `latest.json`

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- The `tauri build` step gains env: `TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}`. With `createUpdaterArtifacts: true`, the build now emits `PoE Campaign Copilot_<ver>_x64-setup.exe` AND `PoE Campaign Copilot_<ver>_x64-setup.exe.sig` under `target/release/bundle/nsis/`.
- A new step (tag builds only) generates `latest.json`:
  ```
  {
    "version": "<tag without v>",
    "pub_date": "<ISO8601>",
    "platforms": {
      "windows-x86_64": {
        "signature": "<contents of the .sig file>",
        "url": "https://github.com/andrewli8/poe-campaign-copilot/releases/download/<tag>/PoE.Campaign.Copilot_<ver>_x64-setup.exe"
      }
    }
  }
  ```
  Use a `pwsh` step: read the tag (`${{ github.ref_name }}`), strip a leading `v` for the version, read the `.sig` file text, build the JSON (note the URL uses the dot-form asset name GitHub serves — verify the exact published asset name format against how the existing artifact uploads; reconcile spaces-vs-dots in the URL, since GitHub replaces spaces with dots in asset download URLs). Write `latest.json`.
- The `softprops/action-gh-release` `files:` list gains the `.exe`, the existing `.sha256.txt`, the `.exe.sig`, AND `latest.json`. Keep the artifact upload for dispatch runs too.
- Keep signing env available to the whole job (or at least the build + any step that needs it).

- [ ] **Step 1:** Rewrite the workflow per the above. Validate YAML well-formedness. Cross-check the asset URL naming against the current release (v0.1.1 published `PoE.Campaign.Copilot_0.1.1_x64-setup.exe` — dots, not spaces — so the `latest.json` `url` must use that dot form).
- [ ] **Step 2:** Commit: `ci: sign release bundles and publish latest.json update manifest`. (Controller will dispatch/tag and verify the real signed artifacts + manifest — that is the true test; be ready for one iteration if the sig path or URL form is off.)

---

### Task 3: Settings update banner + update-and-restart

**Files:**
- Create: `src/useUpdater.ts`, `src/UpdateBanner.tsx`, `src/UpdateBanner.test.tsx`
- Modify: `src/SettingsContainer.tsx` (invoke the check on mount, render the banner), `src/SettingsPage.tsx` (slot for the banner), `src/types.ts` if a shared type helps

**Interfaces:**
- `useUpdater()` hook: on mount, calls `check()` from `@tauri-apps/plugin-updater`. State machine: `idle | checking | available | none | downloading | error`, plus `version: string | null`, `progressPct: number | null`, `error: string | null`, and `installAndRestart: () => Promise<void>`. `installAndRestart` calls `update.downloadAndInstall((event) => …)` tracking progress, then `relaunch()` from `@tauri-apps/plugin-process`. The hook is the untested integration seam (like `useOverlay`) — keep all Tauri-plugin calls inside it.
- `UpdateBanner` — PURE presentational, props `{ status, version, progressPct, error, onUpdate }`:
  - `available` → "Version {version} is available." + an **Update and restart** button (calls `onUpdate`).
  - `downloading` → progress text/bar (`progressPct`), button disabled.
  - `error` → a small non-blocking error line ("Update check failed"), never blocks using Settings.
  - `checking` / `none` / `idle` → render nothing (no banner).
- `SettingsContainer` wires `useUpdater()` and renders `<UpdateBanner .../>` at the top of the settings content (above the existing form). The check firing on container mount == "when the Settings window opens", satisfying the passive-only timing. Add a code comment stating that.
- Tests (`UpdateBanner.test.tsx`, ≥5): available renders version + enabled button; clicking button calls `onUpdate`; downloading shows progress and disables the button; error renders the error line and NOT the button; checking/none/idle render nothing. (Pure component, fixture props — no plugin mocking, matching the project's frontend test convention.)

- [ ] **Step 1:** Failing vitest → RED → implement hook + banner + wiring → GREEN.
- [ ] **Step 2:** `npm test`, `npm run build`, full Rust gate (unchanged) green. Brief mac `npm run tauri dev`: open the settings route; in dev the updater `check()` will error (no signed release reachable / dev has no updater endpoint resolution) — confirm the error path renders the non-blocking line and Settings remains fully usable, NOT a crash. Record this dev-behavior observation.
- [ ] **Step 3:** Commit: `feat: settings update banner with download-and-restart`

---

### Task 4: Docs + updater-baseline release

**Files:**
- Modify: `README.md`

- [ ] **Step 1:** README: add an "Updating" subsection — from the first updater-enabled release (v0.1.2+), opening Settings checks for updates and offers a one-click "Update and restart"; explain the bootstrap (install v0.1.2 manually once; later versions auto-update). Humanized style, no em/en dashes (`grep -c '—\|–' README.md` stays 0). Note that the check only happens when you open Settings, consistent with the no-network-during-play design.
- [ ] **Step 2:** Full gate. Commit: `docs: document the auto-update flow`.
- [ ] **Step 3 (controller-run, not a subagent step):** bump version to 0.1.2 (tauri.conf.json, src-tauri/Cargo.toml, package.json, Cargo.lock), commit, tag `v0.1.2`, push tag, watch the signed release build, and VERIFY: the release has the `.exe`, `.exe.sig`, `.sha256.txt`, and `latest.json`; download `latest.json` and confirm its `version` is `0.1.2`, its `url` resolves to the published exe, and its `signature` is non-empty. This is the artifact that makes future auto-updates work.

---

## Verification (end of plan)

- [ ] Full gate green (Rust + vitest).
- [ ] Dispatched/tagged release produces a signed installer + valid `latest.json` (version, url, signature all correct).
- [ ] Dev-mode: opening Settings with no reachable update degrades to a non-blocking error line, Settings stays usable.
- [ ] README documents the flow and the one-time manual bootstrap.

## Self-Review Notes

- Spec alignment: this adds the ONE deliberate network capability to an otherwise offline app, tightly scoped (Settings-open only, GitHub Releases only, Rust-plugin not webview-fetch, CSP unchanged). Called out explicitly so the security posture stays honest.
- The updater cannot retroactively update v0.1.0/v0.1.1 (no updater embedded) — the bootstrap note is essential and documented.
- Signing private key never enters the repo or this plan; only the public key is committed. The secret is already set in the repo.
- Deferred: delta/differential updates, changelog display in the banner (could parse the release body later), macOS/Linux update artifacts (Windows-only distribution for now), silent/background auto-install (explicitly NOT wanted — user chooses via the button).
