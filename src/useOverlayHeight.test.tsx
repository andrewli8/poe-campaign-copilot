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
