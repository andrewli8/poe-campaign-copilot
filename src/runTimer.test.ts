import { describe, expect, it } from "vitest";
import {
  IDLE_RUN_TIMER,
  elapsedMs,
  formatElapsed,
  isRunning,
  type RunTimerState,
} from "./runTimer";

describe("formatElapsed", () => {
  it("formats zero", () => {
    expect(formatElapsed(0)).toBe("0:00:00");
  });

  it("formats sub-minute values with zero-padded seconds", () => {
    expect(formatElapsed(42_000)).toBe("0:00:42");
    expect(formatElapsed(9_000)).toBe("0:00:09");
  });

  it("rolls minutes into hours", () => {
    expect(formatElapsed(59 * 60_000 + 59_000)).toBe("0:59:59");
    expect(formatElapsed(60 * 60_000)).toBe("1:00:00");
  });

  it("leaves hours unpadded and uncapped past ten hours", () => {
    expect(formatElapsed(12 * 3_600_000 + 3 * 60_000 + 9_000)).toBe("12:03:09");
  });

  it("clamps negative input to zero", () => {
    expect(formatElapsed(-5_000)).toBe("0:00:00");
  });

  it("truncates fractional seconds rather than rounding up", () => {
    expect(formatElapsed(999)).toBe("0:00:00");
    expect(formatElapsed(1_999)).toBe("0:00:01");
  });
});

describe("elapsedMs", () => {
  it("is zero for the never-started state", () => {
    expect(elapsedMs(IDLE_RUN_TIMER, 1_000_000)).toBe(0);
  });

  it("is accumulated time while paused", () => {
    const paused: RunTimerState = { accumulated_ms: 90_000, running_since_ms: null };
    expect(elapsedMs(paused, 1_000_000)).toBe(90_000);
  });

  it("adds the live stretch while running", () => {
    const running: RunTimerState = { accumulated_ms: 60_000, running_since_ms: 500_000 };
    expect(elapsedMs(running, 510_000)).toBe(70_000);
  });

  it("clamps a future running_since (clock skew) to the accumulated time", () => {
    const skewed: RunTimerState = { accumulated_ms: 60_000, running_since_ms: 900_000 };
    expect(elapsedMs(skewed, 800_000)).toBe(60_000);
  });
});

describe("isRunning", () => {
  it("distinguishes running from paused and never-started", () => {
    expect(isRunning(IDLE_RUN_TIMER)).toBe(false);
    expect(isRunning({ accumulated_ms: 5_000, running_since_ms: null })).toBe(false);
    expect(isRunning({ accumulated_ms: 0, running_since_ms: 123 })).toBe(true);
  });
});
