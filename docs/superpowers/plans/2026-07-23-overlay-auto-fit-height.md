# Overlay Auto-Fit Height Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the overlay window's height follow its rendered content automatically so users never have to resize to read clipped information.

**Architecture:** A `ResizeObserver` on the overlay's root element measures rendered height and sends it (debounced, clamped) to a new `set_overlay_height` Tauri command, which resizes the `main` window to `(current width, measured height)` with the top edge pinned. The old fixed-per-mode height logic (`overlay_target_height` and the `set_size` blocks in the zoom/compact toggles) is removed; toggling a mode just changes content, and the window follows.

**Tech Stack:** Rust + Tauri 2 (backend window sizing), React 18 + TypeScript (frontend measurement hook), Vitest + jsdom (frontend tests), `cargo test` (Rust tests).

## Global Constraints

- Height clamp range: `[36, 600]` logical pixels (`MIN_OVERLAY_HEIGHT = 36`, `MAX_OVERLAY_HEIGHT = 600`). This exact pair is duplicated in Rust, in `src/overlayHeight.ts`, and as the CSS `max-height`; a comment in each ties them together.
- Debounce interval: 80ms trailing.
- Grow anchor: top edge pinned (position unchanged; only height changes via `set_size`).
- Width is never modified by any code in this plan — width and screen position stay user-controlled via Setup Mode.
- Immutability, small focused files, explicit error handling per the repo's coding-style rules.
- Reference spec: `docs/superpowers/specs/2026-07-23-overlay-auto-fit-height-design.md`.

---

## File Structure

- `src-tauri/src/main.rs` (modify) — new `set_overlay_height` command + `overlay_height_in_range` helper + height constants; register the command; delete `overlay_target_height`; strip the resize blocks and lock-order dance out of `toggle_zoom_impl` / `toggle_compact_impl`.
- `src/overlayHeight.ts` (create) — pure clamp helper and the shared `MIN`/`MAX` constants. No React, no Tauri — trivially unit-testable.
- `src/overlayHeight.test.ts` (create) — tests for the clamp helper.
- `src/useOverlayHeight.ts` (create) — React hook: observe an element, debounce, clamp, skip-if-unchanged, invoke the command, clean up.
- `src/useOverlayHeight.test.tsx` (create) — hook tests with a fake `ResizeObserver` and an injected `send` spy.
- `src/App.tsx` (modify) — attach the hook to the overlay's root wrapper `<div>`.
- `src/FilmstripBar.css` (modify) — cap `.filmstrip` at `max-height: 600px; overflow-y: auto` so over-cap content scrolls internally.

---

## Task 1: Backend `set_overlay_height` command

**Files:**
- Modify: `src-tauri/src/main.rs` — add constants + helper + command near `overlay_target_height` (around line 397); register the command in `generate_handler!` (around line 675).
- Test: `src-tauri/src/main.rs` `mod tests` (around line 894).

**Interfaces:**
- Produces:
  - `const MIN_OVERLAY_HEIGHT: f64 = 36.0;`
  - `const MAX_OVERLAY_HEIGHT: f64 = 600.0;`
  - `fn overlay_height_in_range(height: f64) -> bool`
  - `#[tauri::command] fn set_overlay_height(app: tauri::AppHandle, height: f64) -> Result<(), String>` — IPC name `set_overlay_height`, argument key `height`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/main.rs`:

```rust
    #[test]
    fn overlay_height_in_range_accepts_bounds_and_interior() {
        assert!(overlay_height_in_range(MIN_OVERLAY_HEIGHT));
        assert!(overlay_height_in_range(MAX_OVERLAY_HEIGHT));
        assert!(overlay_height_in_range(150.0));
    }

    #[test]
    fn overlay_height_in_range_rejects_out_of_range_and_non_finite() {
        assert!(!overlay_height_in_range(MIN_OVERLAY_HEIGHT - 0.1));
        assert!(!overlay_height_in_range(MAX_OVERLAY_HEIGHT + 0.1));
        assert!(!overlay_height_in_range(f64::NAN));
        assert!(!overlay_height_in_range(f64::INFINITY));
        assert!(!overlay_height_in_range(-1.0));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p poe-copilot-app overlay_height_in_range 2>&1 | tail -20`
Expected: compile error — `cannot find function overlay_height_in_range` / `cannot find value MIN_OVERLAY_HEIGHT`.

- [ ] **Step 3: Write minimal implementation**

In `src-tauri/src/main.rs`, immediately ABOVE `fn overlay_target_height` (around line 397), add:

```rust
/// Clamp range for the content-driven overlay height, in logical pixels.
/// Duplicated in `src/overlayHeight.ts` and as `.filmstrip { max-height }`
/// in `src/FilmstripBar.css` — keep all three in step.
const MIN_OVERLAY_HEIGHT: f64 = 36.0;
const MAX_OVERLAY_HEIGHT: f64 = 600.0;

/// True only for a finite height inside `[MIN_OVERLAY_HEIGHT,
/// MAX_OVERLAY_HEIGHT]`. `set_overlay_height` is an IPC boundary, so a
/// NaN/infinite/out-of-range value from the webview is rejected rather
/// than passed to `set_size`.
fn overlay_height_in_range(height: f64) -> bool {
    height.is_finite() && (MIN_OVERLAY_HEIGHT..=MAX_OVERLAY_HEIGHT).contains(&height)
}

/// Resize the overlay to a content-measured height. Width is read back
/// from the current window and left unchanged, so only the bottom edge
/// moves (top-pinned growth). Out-of-range heights are rejected; a missing
/// window (shutdown race) is a no-op success.
#[tauri::command]
fn set_overlay_height(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    if !overlay_height_in_range(height) {
        let msg = format!("set_overlay_height: rejected out-of-range height {height}");
        eprintln!("{msg}");
        return Err(msg);
    }
    let Some(win) = app.get_webview_window("main") else {
        return Ok(());
    };
    if let (Ok(scale), Ok(size)) = (win.scale_factor(), win.outer_size()) {
        let logical = size.to_logical::<f64>(scale);
        if let Err(e) = win.set_size(tauri::LogicalSize::new(logical.width, height)) {
            eprintln!("set_overlay_height: failed to resize window: {e}");
            return Err(e.to_string());
        }
    }
    Ok(())
}
```

Then register it in the `generate_handler!` list (around line 675), adding `set_overlay_height,` after `set_overlay_opacity,`:

```rust
            set_overlay_opacity,
            set_overlay_height,
            get_run_timer
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p poe-copilot-app overlay_height_in_range 2>&1 | tail -20`
Expected: PASS — `test result: ok. 2 passed`.

- [ ] **Step 5: Verify the whole crate still builds and tests pass**

Run: `cargo test -p poe-copilot-app 2>&1 | tail -15`
Expected: all tests pass; no `unused` warning for `set_overlay_height` (it is now registered).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: add set_overlay_height command for content-driven resizing"
```

---

## Task 2: Remove fixed-height logic from the toggles

**Files:**
- Modify: `src-tauri/src/main.rs` — delete `overlay_target_height` (around line 397–409); rewrite `toggle_zoom_impl` (around line 411) and `toggle_compact_impl` (around line 451) to drop the `set_size` blocks, the cross-flag snapshot reads, and the lock-order comments.

**Interfaces:**
- Consumes: nothing new.
- Produces: `toggle_zoom_impl` / `toggle_compact_impl` keep the same signatures (`fn(&tauri::AppHandle) -> bool`) and behavior *except* they no longer resize the window. The `zoom` / `compact` flags, tray-check sync, and `"zoom"` / `"compact"` events are unchanged — the frontend re-render they trigger is what now drives the resize (via Task 4/5).

- [ ] **Step 1: Delete `overlay_target_height`**

Remove the entire function and its doc comment (around line 397–409):

```rust
/// Window height for the overlay given the current compact/zoom flags.
/// Compact takes precedence over zoom: a slim compact bar stays slim even
/// while zoom is also on, so `toggle_zoom_impl` and `toggle_compact_impl`
/// always agree on the target height instead of fighting each other.
fn overlay_target_height(compact: bool, zoom: bool) -> f64 {
    if compact {
        44.0
    } else if zoom {
        420.0
    } else {
        150.0
    }
}
```

(Leave the `MIN_OVERLAY_HEIGHT` / `MAX_OVERLAY_HEIGHT` / `overlay_height_in_range` / `set_overlay_height` items from Task 1 in place — they sit just above this and stay.)

- [ ] **Step 2: Rewrite `toggle_zoom_impl`**

Replace the whole function (around line 411–443) with:

```rust
fn toggle_zoom_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let new_zoom = {
        let mut zoom = state.zoom.lock().unwrap();
        *zoom = !*zoom;
        *zoom
    };
    if let Some(item) = state.zoom_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_zoom);
    }
    // The window is not resized here: flipping `zoom` re-renders the
    // overlay (via the "zoom" event below), its content height changes,
    // and the frontend's ResizeObserver drives `set_overlay_height`.
    let _ = app.emit("zoom", new_zoom);
    new_zoom
}
```

- [ ] **Step 3: Rewrite `toggle_compact_impl`**

Replace the whole function (around line 451–476, the doc comment through the closing brace) with:

```rust
/// Flips `AppState.compact` and re-renders the overlay. The window height
/// is not set here — the "compact" event re-renders the bar, and the
/// frontend's ResizeObserver resizes the window to the new content
/// (see `set_overlay_height`).
fn toggle_compact_impl(app: &tauri::AppHandle) -> bool {
    let state: State<AppState> = app.state();
    let new_compact = {
        let mut compact = state.compact.lock().unwrap();
        *compact = !*compact;
        *compact
    };
    if let Some(item) = state.compact_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(new_compact);
    }
    let _ = app.emit("compact", new_compact);
    new_compact
}
```

- [ ] **Step 4: Verify it builds with no warnings**

Run: `cargo build -p poe-copilot-app 2>&1 | tail -20`
Expected: builds clean. No `unused function overlay_target_height`, no `unused variable`, no unused-import warnings. If `State` or `tauri::LogicalSize` become unused, remove the now-dead `use`/imports the compiler flags.

- [ ] **Step 5: Verify tests still pass**

Run: `cargo test -p poe-copilot-app 2>&1 | tail -15`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "refactor: drop fixed per-mode heights; toggles no longer resize"
```

---

## Task 3: Frontend clamp helper

**Files:**
- Create: `src/overlayHeight.ts`
- Test: `src/overlayHeight.test.ts`

**Interfaces:**
- Produces:
  - `export const MIN_OVERLAY_HEIGHT = 36;`
  - `export const MAX_OVERLAY_HEIGHT = 600;`
  - `export function clampOverlayHeight(height: number): number` — non-finite → `MIN_OVERLAY_HEIGHT`; otherwise clamped into `[MIN, MAX]`.

- [ ] **Step 1: Write the failing test**

Create `src/overlayHeight.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  MAX_OVERLAY_HEIGHT,
  MIN_OVERLAY_HEIGHT,
  clampOverlayHeight,
} from "./overlayHeight";

describe("clampOverlayHeight", () => {
  it("returns interior values unchanged", () => {
    expect(clampOverlayHeight(150)).toBe(150);
  });

  it("clamps below the floor up to the minimum", () => {
    expect(clampOverlayHeight(10)).toBe(MIN_OVERLAY_HEIGHT);
    expect(clampOverlayHeight(0)).toBe(MIN_OVERLAY_HEIGHT);
    expect(clampOverlayHeight(-50)).toBe(MIN_OVERLAY_HEIGHT);
  });

  it("clamps above the ceiling down to the maximum", () => {
    expect(clampOverlayHeight(5000)).toBe(MAX_OVERLAY_HEIGHT);
  });

  it("maps non-finite input to the minimum", () => {
    expect(clampOverlayHeight(Number.NaN)).toBe(MIN_OVERLAY_HEIGHT);
    expect(clampOverlayHeight(Number.POSITIVE_INFINITY)).toBe(MIN_OVERLAY_HEIGHT);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- src/overlayHeight.test.ts 2>&1 | tail -15`
Expected: FAIL — cannot resolve `./overlayHeight`.

- [ ] **Step 3: Write minimal implementation**

Create `src/overlayHeight.ts`:

```ts
// Clamp range for the content-driven overlay height, in logical pixels.
// Duplicated in src-tauri/src/main.rs (MIN/MAX_OVERLAY_HEIGHT) and as
// .filmstrip { max-height } in FilmstripBar.css — keep all three in step.
export const MIN_OVERLAY_HEIGHT = 36;
export const MAX_OVERLAY_HEIGHT = 600;

// Non-finite input (a stray NaN from a mid-layout measurement) collapses to
// the floor rather than propagating to the resize command.
export function clampOverlayHeight(height: number): number {
  if (!Number.isFinite(height)) {
    return MIN_OVERLAY_HEIGHT;
  }
  return Math.min(MAX_OVERLAY_HEIGHT, Math.max(MIN_OVERLAY_HEIGHT, height));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- src/overlayHeight.test.ts 2>&1 | tail -15`
Expected: PASS — 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/overlayHeight.ts src/overlayHeight.test.ts
git commit -m "feat: add clampOverlayHeight helper for overlay auto-fit"
```

---

## Task 4: `useOverlayHeight` hook

**Files:**
- Create: `src/useOverlayHeight.ts`
- Test: `src/useOverlayHeight.test.tsx`

**Interfaces:**
- Consumes: `clampOverlayHeight` from `./overlayHeight`; `invoke` from `@tauri-apps/api/core`.
- Produces:
  - `export type SendHeight = (height: number) => void;`
  - `export function useOverlayHeight(ref: React.RefObject<HTMLElement | null>, opts?: { send?: SendHeight; debounceMs?: number }): void`
  - Default `send` calls `invoke("set_overlay_height", { height })`. Default `debounceMs` is 80. The `send` is injectable so tests need no module mock.

- [ ] **Step 1: Write the failing test**

Create `src/useOverlayHeight.test.tsx`:

```tsx
import { render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MAX_OVERLAY_HEIGHT, MIN_OVERLAY_HEIGHT } from "./overlayHeight";
import { useOverlayHeight } from "./useOverlayHeight";

// Captures the observer callback so tests can drive resize events by hand.
let observerCb: ResizeObserverCallback | null = null;
let disconnectSpy: ReturnType<typeof vi.fn>;

class FakeResizeObserver {
  constructor(cb: ResizeObserverCallback) {
    observerCb = cb;
  }
  observe() {}
  unobserve() {}
  disconnect() {
    disconnectSpy();
  }
}

function fireResize(height: number) {
  observerCb?.(
    [
      {
        borderBoxSize: [{ blockSize: height, inlineSize: 0 }],
        contentRect: { height } as DOMRectReadOnly,
      } as unknown as ResizeObserverEntry,
    ],
    {} as ResizeObserver,
  );
}

function Harness({ send }: { send: (h: number) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useOverlayHeight(ref, { send, debounceMs: 80 });
  return <div ref={ref} />;
}

beforeEach(() => {
  observerCb = null;
  disconnectSpy = vi.fn();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("useOverlayHeight", () => {
  it("debounces a burst into a single send with the final height", () => {
    const send = vi.fn();
    render(<Harness send={send} />);
    fireResize(100);
    fireResize(150);
    fireResize(200);
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(80);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(200);
  });

  it("clamps the measured height before sending", () => {
    const send = vi.fn();
    render(<Harness send={send} />);
    fireResize(5000);
    vi.advanceTimersByTime(80);
    expect(send).toHaveBeenCalledWith(MAX_OVERLAY_HEIGHT);

    fireResize(5);
    vi.advanceTimersByTime(80);
    expect(send).toHaveBeenLastCalledWith(MIN_OVERLAY_HEIGHT);
  });

  it("does not re-send an unchanged clamped height", () => {
    const send = vi.fn();
    render(<Harness send={send} />);
    fireResize(200);
    vi.advanceTimersByTime(80);
    fireResize(200);
    vi.advanceTimersByTime(80);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("disconnects the observer and clears the timer on unmount", () => {
    const send = vi.fn();
    const { unmount } = render(<Harness send={send} />);
    fireResize(200);
    unmount();
    vi.advanceTimersByTime(80);
    expect(disconnectSpy).toHaveBeenCalledTimes(1);
    expect(send).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- src/useOverlayHeight.test.tsx 2>&1 | tail -15`
Expected: FAIL — cannot resolve `./useOverlayHeight`.

- [ ] **Step 3: Write minimal implementation**

Create `src/useOverlayHeight.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";
import type { RefObject } from "react";
import { clampOverlayHeight } from "./overlayHeight";

export type SendHeight = (height: number) => void;

const defaultSend: SendHeight = (height) => {
  void invoke("set_overlay_height", { height });
};

// Measures `ref`'s rendered height with a ResizeObserver and pushes it to
// the backend `set_overlay_height` command, debounced and clamped. Skips
// the call when the clamped height matches the last one sent, so a settle
// that lands on the same value does not spam IPC. `send` is injectable for
// tests; production uses the Tauri invoke.
export function useOverlayHeight(
  ref: RefObject<HTMLElement | null>,
  opts: { send?: SendHeight; debounceMs?: number } = {},
): void {
  const { send = defaultSend, debounceMs = 80 } = opts;

  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") {
      return;
    }

    let lastSent: number | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      const raw = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
      const height = clampOverlayHeight(raw);
      if (timer !== null) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = null;
        if (lastSent === height) {
          return;
        }
        lastSent = height;
        send(height);
      }, debounceMs);
    });

    observer.observe(el);
    return () => {
      observer.disconnect();
      if (timer !== null) {
        clearTimeout(timer);
      }
    };
  }, [ref, send, debounceMs]);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- src/useOverlayHeight.test.tsx 2>&1 | tail -15`
Expected: PASS — 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/useOverlayHeight.ts src/useOverlayHeight.test.tsx
git commit -m "feat: add useOverlayHeight hook (observe, debounce, clamp, send)"
```

---

## Task 5: Wire the hook into the overlay and cap the filmstrip

**Files:**
- Modify: `src/App.tsx` — attach `useOverlayHeight` to the opacity wrapper `<div>`.
- Modify: `src/FilmstripBar.css` — cap `.filmstrip` height and enable internal scroll.

**Interfaces:**
- Consumes: `useOverlayHeight` from `./useOverlayHeight`.
- Produces: nothing new — this is the integration seam.

- [ ] **Step 1: Attach the hook in `App.tsx`**

Rewrite `src/App.tsx` to add a ref on the wrapper and call the hook:

```tsx
import { useRef } from "react";
import { FilmstripBar } from "./FilmstripBar";
import { useOverlay } from "./useOverlay";
import { useOverlayHeight } from "./useOverlayHeight";

export default function App() {
  const { model, zoom, setupMode, compact, overlayOpacity, runTimer, showRunTimer, nowMs } =
    useOverlay();
  const rootRef = useRef<HTMLDivElement>(null);
  // Measure the overlay's rendered height and resize the window to match.
  // Attaching before the early `return null` is not allowed (hooks must be
  // unconditional), so the ref is live only once `model` renders; the
  // observer simply starts on the first real frame.
  useOverlayHeight(rootRef);
  if (!model) return null;
  // Opacity is applied here via CSS on the overlay's root wrapper (rather
  // than a native window-opacity API, which Tauri v2 does not expose
  // cross-platform): the window itself is already transparent, so fading
  // the webview content is equivalent and portable. The value is clamped
  // to a 20% floor (frontend and backend) so the overlay can never be
  // faded into unfindability.
  return (
    <div ref={rootRef} style={{ opacity: overlayOpacity }}>
      <FilmstripBar
        model={model}
        zoom={zoom}
        setupMode={setupMode}
        compact={compact}
        runTimer={runTimer}
        showRunTimer={showRunTimer}
        nowMs={nowMs}
      />
    </div>
  );
}
```

Note: `useOverlayHeight` is called before the `if (!model) return null;` guard so the hook order is unconditional (React rule of hooks). The ref attaches on the first frame that renders the wrapper, and the observer begins then.

- [ ] **Step 2: Cap the filmstrip in CSS**

In `src/FilmstripBar.css`, add two properties to the `.filmstrip` rule (the block starting at line 12). After the existing `border-radius: 6px;` line, add:

```css
  /* Cap must match MAX_OVERLAY_HEIGHT in overlayHeight.ts / main.rs. Once
     content exceeds the window cap, scroll inside the bar instead of
     letting the window clip it. */
  max-height: 600px;
  overflow-y: auto;
```

The `.filmstrip` rule then reads (context — do not duplicate, just insert the two properties):

```css
.filmstrip {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  background: rgba(10, 10, 12, 0.82);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  max-height: 600px;
  overflow-y: auto;
  color: #ffffff;
  font-family: -apple-system, "Segoe UI", system-ui, sans-serif;
  font-size: 13px;
  -webkit-user-select: none;
  user-select: none;
}
```

- [ ] **Step 3: Verify the frontend builds and all tests pass**

Run: `npm run build 2>&1 | tail -15`
Expected: `tsc` type-checks clean and `vite build` succeeds (no unused-import or type errors from the `App.tsx` edit).

Run: `npm test 2>&1 | tail -8`
Expected: all test files pass, including the new `overlayHeight` and `useOverlayHeight` suites and the unchanged `FilmstripBar` / `SettingsPage` suites.

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx src/FilmstripBar.css
git commit -m "feat: drive overlay window height from rendered content"
```

---

## Task 6: Full verification

**Files:** none (verification only).

- [ ] **Step 1: Rust workspace tests**

Run: `cargo test --workspace 2>&1 | grep -E "test result" | grep -v " 0 failed"; echo "failing-suites: $?"`
Expected: prints nothing and `failing-suites: 1` (grep found no failing-result lines).

- [ ] **Step 2: Frontend tests**

Run: `npm test 2>&1 | grep -E "Test Files|Tests "`
Expected: all files passed; total tests = previous count + 8 new (4 clamp + 4 hook).

- [ ] **Step 3: Rust build is warning-clean**

Run: `cargo build -p poe-copilot-app 2>&1 | grep -E "warning|error"; echo "issues: $?"`
Expected: no `overlay_target_height`, no unused `State`/`LogicalSize`; `issues: 1` (grep found nothing).

- [ ] **Step 4: Manual smoke (documented, run if a display is available)**

Two-terminal fake-play demo from the README:

```bash
rm -f /tmp/fake-client.txt && touch /tmp/fake-client.txt
POE_COPILOT_LOG=/tmp/fake-client.txt POE_COPILOT_LOG_REPLAY=1 npm run tauri dev
```

Then in a second terminal:

```bash
cargo run -p replay --bin fake-play -- crates/replay/fixtures/act1-opening.log /tmp/fake-client.txt 800
```

Confirm: the window height tracks content as zones change; toggling Zoom (`Alt+Shift+Z`) and Compact (`Alt+Shift+C`) grows/shrinks the window with no clipping and no flicker; if a zone's content exceeds ~600px, the bar scrolls internally rather than being cut off. Stop with the tray's Quit and `rm -f /tmp/fake-client.txt`.

---

## Self-Review Notes

- **Spec coverage:** content-driven height (Tasks 4–5), `set_overlay_height` + validation (Task 1), removal of `overlay_target_height` and toggle resizes + lock-order cleanup (Task 2), clamp `[36,600]` (Tasks 1, 3), debounce 80ms + skip-if-unchanged (Task 4), internal scroll cap (Task 5), top-pinned growth (Task 1 keeps width, moves only height), error handling at IPC/missing-window/observer-absent (Tasks 1, 4). All spec sections map to a task.
- **Type consistency:** `set_overlay_height(app, height)` / IPC key `height` used identically in Rust (Task 1) and the hook's default `send` (Task 4). `clampOverlayHeight` / `MIN_OVERLAY_HEIGHT` / `MAX_OVERLAY_HEIGHT` names match across Tasks 3–4. `useOverlayHeight(ref, { send, debounceMs })` signature matches its test harness and the `App.tsx` call site.
- **Out of scope (per spec):** width auto-fit, grow-away-from-edge anchoring, animated resize.
