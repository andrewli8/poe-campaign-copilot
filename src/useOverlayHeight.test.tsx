import { render } from "@testing-library/react";
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
  const setRoot = useOverlayHeight({ send, debounceMs: 80 });
  return <div ref={setRoot} />;
}

// Mirrors App: renders null first (model not loaded), mounts the observed
// node on a later render.
function LateHarness({ show, send }: { show: boolean; send: (h: number) => void }) {
  const setRoot = useOverlayHeight({ send, debounceMs: 80 });
  return show ? <div ref={setRoot} /> : null;
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

  it("attaches when the observed node mounts on a later render", () => {
    const send = vi.fn();
    const { rerender } = render(<LateHarness show={false} send={send} />);
    // Nothing to observe yet — mirrors App returning null while model loads.
    expect(observerCb).toBeNull();
    rerender(<LateHarness show={true} send={send} />);
    // The node mounted; the callback ref must have attached the observer.
    expect(observerCb).not.toBeNull();
    fireResize(200);
    vi.advanceTimersByTime(80);
    expect(send).toHaveBeenCalledWith(200);
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
