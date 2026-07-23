import { describe, expect, it } from "vitest";
import {
  OPACITY_DEFAULT,
  OPACITY_MAX,
  OPACITY_MIN,
  clampOpacity,
} from "./opacity";

describe("opacity constants", () => {
  it("floors the minimum at 20% so the overlay can never become invisible", () => {
    expect(OPACITY_MIN).toBe(0.2);
  });

  it("caps at fully opaque and defaults to fully opaque", () => {
    expect(OPACITY_MAX).toBe(1);
    expect(OPACITY_DEFAULT).toBe(1);
  });
});

describe("clampOpacity", () => {
  it("passes through in-range values", () => {
    expect(clampOpacity(0.5)).toBe(0.5);
    expect(clampOpacity(OPACITY_MIN)).toBe(OPACITY_MIN);
    expect(clampOpacity(OPACITY_MAX)).toBe(OPACITY_MAX);
  });

  it("clamps values below the floor and above the cap", () => {
    expect(clampOpacity(0)).toBe(OPACITY_MIN);
    expect(clampOpacity(-3)).toBe(OPACITY_MIN);
    expect(clampOpacity(2)).toBe(OPACITY_MAX);
  });

  it("degrades non-finite input to the default", () => {
    expect(clampOpacity(Number.NaN)).toBe(OPACITY_DEFAULT);
    expect(clampOpacity(Number.POSITIVE_INFINITY)).toBe(OPACITY_DEFAULT);
  });
});
