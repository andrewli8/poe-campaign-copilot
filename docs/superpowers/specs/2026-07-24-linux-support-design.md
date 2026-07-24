# Linux Support — Design

**Date:** 2026-07-24
**Status:** Approved, implemented on `feat/linux-support`.

## Goal

Make the overlay build and run correctly on Linux, and have tagged releases
produce an auto-updatable **AppImage** alongside the existing Windows installer.
Windows behavior stays byte-for-byte unchanged; macOS remains source-only (not
in scope here).

## Why this is small

The codebase is already portable — a `grep` for `target_os` / `cfg(windows)` /
`cfg(unix)` across `src-tauri/src` and `crates/` finds a single hit
(`windows_subsystem` on `main.rs:1`, which is already correctly `cfg`-gated).
The default `Client.txt` path is `None` (the user always Browses via the
cross-platform `tauri-plugin-dialog`), so no path-detection code is needed. The
real work is (1) one runtime line, (2) packaging/CI, (3) docs.

## Decisions

- **Scope:** source + packaged release (AppImage). No per-push Linux CI matrix
  (the tag-time release build gives Linux compile coverage).
- **Format:** AppImage only. It is self-contained, cross-distro, and the only
  Linux bundle Tauri's updater supports — so update parity with Windows holds.
  No `.deb`/`.rpm`.
- **Wayland:** force the GTK backend to x11 so the overlay always runs through
  XWayland, even on native Wayland sessions.

## Design

### 1. Runtime — force XWayland (`src-tauri/src/main.rs`)

At the top of `main()`, before the Tauri builder (hence before any GTK init),
Linux-only:

```rust
#[cfg(target_os = "linux")]
// SAFETY: called at the very start of `main`, before any threads spawn.
unsafe {
    std::env::set_var("GDK_BACKEND", "x11");
}
```

Rationale: transparency, always-on-top, skip-taskbar, and `tauri-plugin-global-
shortcut`'s X11 grabs are unsupported under a native Wayland session. Routing
through XWayland (present on virtually every desktop) makes the overlay behave
as it does on X11. `set_var` is `unsafe` under the crate's Rust 2024 edition,
hence the `unsafe` block. We deliberately do **not** honor a pre-set
`GDK_BACKEND` — a Wayland backend silently breaks the overlay.

### 2. Packaging (`src-tauri/tauri.conf.json`)

No change. `targets` stays `["nsis"]`; the Linux CI job passes
`--bundles appimage`, which overrides the config target on that runner only, so
the Windows config is untouched. `createUpdaterArtifacts: true` already emits the
`.AppImage.sig`. Bundled resources (`vendor`, `content/layouts`) and the tray
icon work as-is on Linux (`libayatana-appindicator`).

### 3. CI / Release (`.github/workflows/release.yml`)

`latest.json` carries per-platform keys and must be a single file, so the old
"Windows job builds *and* publishes" shape is refactored into:

- **`build-windows`** (`windows-latest`): builds NSIS, checksums, uploads
  `windows-installer` artifact (`.exe`, `.exe.sig`, `.sha256.txt`). No publish.
- **`build-linux`** (`ubuntu-22.04` — oldest base that ships WebKitGTK 4.1, to
  keep the AppImage's glibc floor low): installs Tauri's documented Debian deps,
  builds `--bundles appimage`, checksums, uploads `linux-appimage` artifact
  (`.AppImage`, `.AppImage.sig`, `.sha256.txt`).
- **`release`** (`ubuntu-22.04`, `needs: [build-windows, build-linux]`,
  tag-gated): downloads both, builds **one** merged `latest.json` with
  `windows-x86_64` + `linux-x86_64` (reusing the space→dot asset-URL rule via
  `jq --rawfile` for the minisign blocks), and publishes the single GitHub
  release with all assets.

Both build jobs run on every trigger (dispatch included) for compile coverage;
only the tag-gated `release` job publishes, so a manual dispatch is a safe dry
run. Net Windows outcome is identical.

### 4. Docs (`README.md`)

- New **"Install on Linux (AppImage)"** section: download, `chmod +x`, run;
  XWayland note; windowed-fullscreen requirement.
- **Run from source** gains the Debian/Ubuntu system-dependency block.
- **Client.txt** common-locations list gains the Steam/Proton Linux paths
  (log lives in the game's install dir, not `compatdata`).
- Caveat and Development notes updated to include Linux.

## Testing / verification

The author has no Linux device, so automated coverage is deliberately maximized
(`.github/workflows/ci.yml`):

- **`rust` + `app` jobs gain `ubuntu-22.04`** — `cargo fmt`/`clippy`/`test
  --workspace` and `cargo check -p poe-copilot-app` now run on Linux every push.
  This is the first place the `cfg(target_os = "linux")` GDK_BACKEND block is
  actually compiled and clippy'd (previously never, off-tag).
- **`linux-smoke` job** — builds the real AppImage (unsigned, via
  `--config '{"bundle":{"createUpdaterArtifacts":false}}'` so no signing secret
  reaches CI) and headlessly launches it under `xvfb-run` + `dbus-run-session`,
  asserting the process survives startup. This exercises Linux packaging + the
  GTK/WebKitGTK/XWayland/tray/window/`setup()` init surface — the closest
  automated proxy for "it runs on Linux". Observed green on GitHub's Linux
  runners (the app reaches `setup()` and logs "overlay will wait for Settings"
  after building the tray/window/webview), so the launch step is a required
  check.
- Pure-logic correctness (route engine, composer, session, …) is OS-independent
  and already covered by the cross-platform `rust` test job.
- Release workflow is dry-runnable via `workflow_dispatch` (build jobs run,
  publish is tag-gated).

- **Still requires a human on real Linux hardware (cannot be automated):**
  the overlay visually drawing over the game, click-through, focus handling, and
  global hotkeys actually grabbing over PoE under XWayland. Realistic path: a
  community beta tester.

## Risks / out of scope

- **Global shortcuts under XWayland**: work in the large majority of setups; a
  few compositors restrict X11 grabs. The `GDK_BACKEND=x11` force is the best
  available mitigation.
- **Out of scope:** `.deb`/`.rpm`, log-path auto-detection, macOS packaging.
  (A per-push Linux CI matrix + AppImage build + xvfb smoke test were added —
  see Testing — because the author has no Linux device to test on.)
