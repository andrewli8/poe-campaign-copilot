import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FilmstripBar, type FilmstripBarProps } from "./FilmstripBar";
import { IDLE_RUN_TIMER } from "./runTimer";
import type { UiModel } from "./types";

const PIXEL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNsb2j4DwAFKAJ003oL8QAAAABJRU5ErkJggg==";

function model(overrides: Partial<UiModel["overlay"]> = {}, extra: Partial<UiModel> = {}): UiModel {
  return {
    overlay: {
      zone_name: "The Coast",
      area_id: "1_1_2",
      act: 1,
      off_route_zone: null,
      layout_images: [{ file: "a.png", stale: false }],
      layout_notes: [{ text: "Follow the right wall.", stale: false }],
      steps_in_zone: ["Get waypoint", "➞ The Mud Flats"],
      sub_hints: ["Go ↗"],
      primary: "Get waypoint",
      next_zone: "The Mud Flats",
      pending_count: 2,
      town_reminders: [],
      build_reminders: [],
      is_town: false,
      route_complete: false,
      location_status: "on_track",
      groups_behind: 0,
      ...overrides,
    },
    images: [{ file: "a.png", stale: false, data_url: PIXEL }],
    waiting_for_log: false,
    build_summary: null,
    ...extra,
  };
}

// Shared render helper: defaults zoom/setupMode/compact to false so each
// test only needs to specify the prop(s) it cares about.
function renderBar(overrides: Partial<FilmstripBarProps> & { model: UiModel }) {
  return render(
    <FilmstripBar
      zoom={false}
      setupMode={false}
      compact={false}
      runTimer={IDLE_RUN_TIMER}
      showRunTimer={true}
      nowMs={1_000_000}
      {...overrides}
    />,
  );
}

describe("FilmstripBar", () => {
  it("renders zone, act, primary, next and pending badge", () => {
    renderBar({ model: model() });
    expect(screen.getByText("The Coast")).toBeInTheDocument();
    expect(screen.getByText(/act 1/i)).toBeInTheDocument();
    expect(screen.getByText("Get waypoint")).toBeInTheDocument();
    expect(screen.getByText(/next: the mud flats/i)).toBeInTheDocument();
    expect(screen.getByText(/2 pending/i)).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", PIXEL);
  });

  it("shows waiting state before any log data", () => {
    renderBar({ model: model({}, { waiting_for_log: true }) });
    expect(screen.getByText(/waiting for client\.txt/i)).toBeInTheDocument();
    expect(screen.queryByText("The Coast")).not.toBeInTheDocument();
  });

  it("shows off-route banner and town reminders in town", () => {
    renderBar({
      model: model({
        off_route_zone: "Lioneye's Watch",
        is_town: true,
        town_reminders: ["Claim quest reward: Quicksilver Flask"],
      }),
    });
    expect(screen.getByText(/off route/i)).toBeInTheDocument();
    expect(screen.getByText(/quicksilver flask/i)).toBeInTheDocument();
  });

  it("shows build reminders with the build class", () => {
    renderBar({
      model: model({
        is_town: true,
        build_reminders: ["Gem available: Frostblink"],
      }),
    });
    const item = screen.getByText(/gem available: frostblink/i);
    expect(item).toHaveClass("build");
  });

  it("marks stale images and notes", () => {
    const m = model({ layout_notes: [{ text: "Old info.", stale: true }] });
    m.images = [{ file: "a.png", stale: true, data_url: PIXEL }];
    renderBar({ model: m });
    expect(screen.getByText(/outdated/i)).toBeInTheDocument();
    expect(screen.getByText("Old info.")).toHaveClass("stale");
  });

  it("renders campaign complete state", () => {
    renderBar({
      model: model({ route_complete: true, zone_name: "Campaign complete" }),
    });
    expect(screen.getByText(/campaign complete/i)).toBeInTheDocument();
  });

  it("applies zoom and setup-mode classes", () => {
    const { container } = renderBar({
      model: model(),
      zoom: true,
      setupMode: true,
    });
    expect(container.firstChild).toHaveClass("zoom");
    expect(screen.getByText(/drag to move/i)).toBeInTheDocument();
  });

  it("renders the build summary when present, with the build-summary class", () => {
    renderBar({
      model: model({}, { build_summary: "Ranger (Deadeye) — 12 milestones" }),
    });
    const summary = screen.getByText("Ranger (Deadeye) — 12 milestones");
    expect(summary).toHaveClass("build-summary");
  });

  it("omits the build summary line when there is no build", () => {
    const { container } = renderBar({ model: model() });
    expect(container.querySelector(".build-summary")).not.toBeInTheDocument();
  });

  // Tauri v2 only starts a window drag when the mousedown TARGET element
  // itself carries data-tauri-drag-region, so a bare attribute on the root
  // does not make clicks on child elements draggable. Setup mode instead
  // renders a dedicated, full-window .drag-layer element that carries the
  // attribute, so a click anywhere in the bar lands on it and drags.
  it("renders the drag layer in the normal-playing state only in setup mode", () => {
    const { container, rerender } = renderBar({
      model: model(),
      setupMode: true,
    });
    const dragLayer = container.querySelector(".drag-layer");
    expect(dragLayer).toBeInTheDocument();
    expect(dragLayer).toHaveAttribute("data-tauri-drag-region");
    // No other element carries the drag attribute — only the dedicated layer.
    expect(container.querySelector("[data-tauri-drag-region]")).toBe(dragLayer);

    rerender(
      <FilmstripBar
        model={model()}
        zoom={false}
        setupMode={false}
        compact={false}
        runTimer={IDLE_RUN_TIMER}
        showRunTimer={true}
        nowMs={1_000_000}
      />,
    );
    expect(container.querySelector(".drag-layer")).not.toBeInTheDocument();
    expect(container.querySelector("[data-tauri-drag-region]")).not.toBeInTheDocument();
  });

  it("renders the drag layer in the waiting state only in setup mode, alongside the setup hint", () => {
    const { container, rerender } = renderBar({
      model: model({}, { waiting_for_log: true }),
      setupMode: true,
    });
    const dragLayer = container.querySelector(".drag-layer");
    expect(dragLayer).toBeInTheDocument();
    expect(dragLayer).toHaveAttribute("data-tauri-drag-region");
    expect(screen.getByText(/drag to move/i)).toBeInTheDocument();

    rerender(
      <FilmstripBar
        model={model({}, { waiting_for_log: true })}
        zoom={false}
        setupMode={false}
        compact={false}
        runTimer={IDLE_RUN_TIMER}
        showRunTimer={true}
        nowMs={1_000_000}
      />,
    );
    expect(container.querySelector(".drag-layer")).not.toBeInTheDocument();
    expect(container.querySelector("[data-tauri-drag-region]")).not.toBeInTheDocument();
    expect(screen.queryByText(/drag to move/i)).not.toBeInTheDocument();
  });

  it("renders the drag layer in the campaign-complete state only in setup mode", () => {
    const { container, rerender } = renderBar({
      model: model({ route_complete: true }),
      setupMode: true,
    });
    const dragLayer = container.querySelector(".drag-layer");
    expect(dragLayer).toBeInTheDocument();
    expect(dragLayer).toHaveAttribute("data-tauri-drag-region");

    rerender(
      <FilmstripBar
        model={model({ route_complete: true })}
        zoom={false}
        setupMode={false}
        compact={false}
        runTimer={IDLE_RUN_TIMER}
        showRunTimer={true}
        nowMs={1_000_000}
      />,
    );
    expect(container.querySelector(".drag-layer")).not.toBeInTheDocument();
    expect(container.querySelector("[data-tauri-drag-region]")).not.toBeInTheDocument();
  });

  describe("compact mode", () => {
    it("renders zone name, primary action, and next-zone arrow only", () => {
      renderBar({ model: model(), compact: true });
      expect(screen.getByText("The Coast")).toBeInTheDocument();
      expect(screen.getByText("Get waypoint")).toBeInTheDocument();
      expect(screen.getByText(/the mud flats/i)).toBeInTheDocument();
    });

    it("omits images, notes, header badges, off-route banner, and build summary", () => {
      const { container } = renderBar({
        model: model(
          {
            off_route_zone: "Lioneye's Watch",
            is_town: true,
            town_reminders: ["Claim quest reward: Quicksilver Flask"],
            build_reminders: ["Gem available: Frostblink"],
          },
          { build_summary: "Ranger (Deadeye) — 12 milestones" },
        ),
        compact: true,
      });
      expect(container.querySelector(".image-row")).not.toBeInTheDocument();
      expect(container.querySelector(".notes-list")).not.toBeInTheDocument();
      expect(container.querySelector(".header-row")).not.toBeInTheDocument();
      expect(container.querySelector(".off-route-banner")).not.toBeInTheDocument();
      expect(container.querySelector(".pending-badge")).not.toBeInTheDocument();
      expect(container.querySelector(".sub-hints")).not.toBeInTheDocument();
      expect(container.querySelector(".town-reminders")).not.toBeInTheDocument();
      expect(container.querySelector(".build-reminders")).not.toBeInTheDocument();
      expect(container.querySelector(".build-summary")).not.toBeInTheDocument();
      expect(screen.queryByRole("img")).not.toBeInTheDocument();
    });

    it("omits the next-zone arrow when there is no next zone", () => {
      const { container } = renderBar({
        model: model({ next_zone: null }),
        compact: true,
      });
      expect(container.querySelector(".compact-next")).not.toBeInTheDocument();
    });

    it("applies the compact root class", () => {
      const { container } = renderBar({ model: model(), compact: true });
      expect(container.firstChild).toHaveClass("compact");
    });

    it("renders the drag layer when compact and setup mode are both on", () => {
      const { container } = renderBar({
        model: model(),
        compact: true,
        setupMode: true,
      });
      const dragLayer = container.querySelector(".drag-layer");
      expect(dragLayer).toBeInTheDocument();
      expect(dragLayer).toHaveAttribute("data-tauri-drag-region");
      expect(container.firstChild).toHaveClass("compact");
    });

    it("does not affect the waiting state", () => {
      renderBar({ model: model({}, { waiting_for_log: true }), compact: true });
      expect(screen.getByText(/waiting for client\.txt/i)).toBeInTheDocument();
    });

    it("does not affect the campaign-complete state", () => {
      renderBar({ model: model({ route_complete: true }), compact: true });
      expect(screen.getByText(/campaign complete/i)).toBeInTheDocument();
    });
  });

  describe("full (non-compact) mode", () => {
    it("still renders the full layout including the image row", () => {
      const { container } = renderBar({ model: model(), compact: false });
      expect(container.querySelector(".image-row")).toBeInTheDocument();
      expect(container.querySelector(".header-row")).toBeInTheDocument();
      expect(container.querySelector(".notes-list")).toBeInTheDocument();
    });
  });

  describe("run timer chip", () => {
    it("shows elapsed time while running", () => {
      renderBar({
        model: model(),
        runTimer: { accumulated_ms: 60_000, running_since_ms: 990_000 },
        nowMs: 1_000_000,
      });
      // 60s accumulated + 10s live stretch
      expect(screen.getByText("0:01:10")).toBeInTheDocument();
    });

    it("shows 0:00:00 when never started", () => {
      renderBar({ model: model() });
      expect(screen.getByText("0:00:00")).toBeInTheDocument();
    });

    it("is hidden when disabled in settings", () => {
      renderBar({ model: model(), showRunTimer: false });
      expect(screen.queryByText("0:00:00")).not.toBeInTheDocument();
    });

    it("is hidden on the waiting screen", () => {
      renderBar({ model: model({}, { waiting_for_log: true }) });
      expect(screen.queryByText("0:00:00")).not.toBeInTheDocument();
    });

    it("marks a paused timer", () => {
      renderBar({
        model: model(),
        runTimer: { accumulated_ms: 90_000, running_since_ms: null },
      });
      const chip = screen.getByText("0:01:30");
      expect(chip.closest(".run-timer")).toHaveClass("paused");
    });

    it("appears in compact mode too", () => {
      renderBar({
        model: model(),
        compact: true,
        runTimer: { accumulated_ms: 5_000, running_since_ms: null },
      });
      expect(screen.getByText("0:00:05")).toBeInTheDocument();
    });
  });
});
