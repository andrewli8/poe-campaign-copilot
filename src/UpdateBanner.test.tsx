import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";
import type { UpdaterStatus } from "./useUpdater";

function noop() {}

function renderBanner(
  overrides: Partial<{
    status: UpdaterStatus;
    version: string | null;
    progressPct: number | null;
    error: string | null;
    onUpdate: () => void;
  }> = {},
) {
  const props = {
    status: "idle" as UpdaterStatus,
    version: null,
    progressPct: null,
    error: null,
    onUpdate: noop,
    ...overrides,
  };
  return render(<UpdateBanner {...props} />);
}

describe("UpdateBanner", () => {
  it("renders the version and an enabled 'Update and restart' button when available", () => {
    renderBanner({ status: "available", version: "0.1.2" });
    expect(screen.getByText(/version 0\.1\.2 is available/i)).toBeInTheDocument();
    const button = screen.getByRole("button", { name: /update and restart/i });
    expect(button).toBeInTheDocument();
    expect(button).toBeEnabled();
  });

  it("calls onUpdate when the button is clicked", () => {
    const onUpdate = vi.fn();
    renderBanner({ status: "available", version: "0.1.2", onUpdate });
    fireEvent.click(screen.getByRole("button", { name: /update and restart/i }));
    expect(onUpdate).toHaveBeenCalledTimes(1);
  });

  it("shows download progress and disables the button while downloading", () => {
    renderBanner({ status: "downloading", version: "0.1.2", progressPct: 42 });
    expect(screen.getByText(/42%/)).toBeInTheDocument();
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("renders a non-blocking error line and no button on error", () => {
    renderBanner({ status: "error", error: "network unreachable" });
    expect(screen.getByText(/update check failed/i)).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders nothing when checking", () => {
    const { container } = renderBanner({ status: "checking" });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when none", () => {
    const { container } = renderBanner({ status: "none" });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when idle", () => {
    const { container } = renderBanner({ status: "idle" });
    expect(container).toBeEmptyDOMElement();
  });
});
