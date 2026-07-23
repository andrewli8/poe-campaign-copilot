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
