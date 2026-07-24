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

- `cargo test --workspace` unchanged and green (the one runtime line is
  `cfg(target_os = "linux")`-gated; no crate logic touched).
- `cargo clippy -D warnings` must accept the new `unsafe` block.
- Release workflow is dry-runnable via `workflow_dispatch` (build jobs run,
  publish is tag-gated).
- **Manual, on real Linux hardware:** overlay transparency/click-through over
  the game, global hotkeys under XWayland, and AppImage launch + in-place
  update. Cannot be verified from a headless dev box.

## Risks / out of scope

- **Global shortcuts under XWayland**: work in the large majority of setups; a
  few compositors restrict X11 grabs. The `GDK_BACKEND=x11` force is the best
  available mitigation.
- **Out of scope:** `.deb`/`.rpm`, per-push Linux CI matrix, log-path
  auto-detection, macOS packaging.
