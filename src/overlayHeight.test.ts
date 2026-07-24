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

  it("rounds fractional measurements UP to a whole pixel", () => {
    // ResizeObserver reports fractional border-box sizes (e.g. 226.375).
    // The window must never be set fractionally SMALLER than the content:
    // on Windows the native height rounds to physical pixels, and a
    // sub-pixel shortfall overflows the document, which grows a
    // layout-consuming scrollbar, reflows the content, and re-triggers
    // the observer — a sustained resize loop. Ceil, never floor/round.
    expect(clampOverlayHeight(226.375)).toBe(227);
    expect(clampOverlayHeight(150.0001)).toBe(151);
    expect(clampOverlayHeight(150.9)).toBe(151);
  });

  it("keeps ceiled values inside the clamp range", () => {
    expect(clampOverlayHeight(MAX_OVERLAY_HEIGHT - 0.5)).toBe(MAX_OVERLAY_HEIGHT);
    expect(clampOverlayHeight(MAX_OVERLAY_HEIGHT + 0.5)).toBe(MAX_OVERLAY_HEIGHT);
    expect(clampOverlayHeight(MIN_OVERLAY_HEIGHT - 0.5)).toBe(MIN_OVERLAY_HEIGHT);
  });
});
