import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FilmstripBar } from "./FilmstripBar";
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
      ...overrides,
    },
    images: [{ file: "a.png", stale: false, data_url: PIXEL }],
    waiting_for_log: false,
    build_summary: null,
    ...extra,
  };
}

describe("FilmstripBar", () => {
  it("renders zone, act, primary, next and pending badge", () => {
    render(<FilmstripBar model={model()} zoom={false} setupMode={false} />);
    expect(screen.getByText("The Coast")).toBeInTheDocument();
    expect(screen.getByText(/act 1/i)).toBeInTheDocument();
    expect(screen.getByText("Get waypoint")).toBeInTheDocument();
    expect(screen.getByText(/next: the mud flats/i)).toBeInTheDocument();
    expect(screen.getByText(/2 pending/i)).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", PIXEL);
  });

  it("shows waiting state before any log data", () => {
    render(
      <FilmstripBar
        model={model({}, { waiting_for_log: true })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/waiting for client\.txt/i)).toBeInTheDocument();
    expect(screen.queryByText("The Coast")).not.toBeInTheDocument();
  });

  it("shows off-route banner and town reminders in town", () => {
    render(
      <FilmstripBar
        model={model({
          off_route_zone: "Lioneye's Watch",
          is_town: true,
          town_reminders: ["Claim quest reward: Quicksilver Flask"],
        })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/off route/i)).toBeInTheDocument();
    expect(screen.getByText(/quicksilver flask/i)).toBeInTheDocument();
  });

  it("shows build reminders with the build class", () => {
    render(
      <FilmstripBar
        model={model({
          is_town: true,
          build_reminders: ["Gem available: Frostblink"],
        })}
        zoom={false}
        setupMode={false}
      />,
    );
    const item = screen.getByText(/gem available: frostblink/i);
    expect(item).toHaveClass("build");
  });

  it("marks stale images and notes", () => {
    const m = model({ layout_notes: [{ text: "Old info.", stale: true }] });
    m.images = [{ file: "a.png", stale: true, data_url: PIXEL }];
    render(<FilmstripBar model={m} zoom={false} setupMode={false} />);
    expect(screen.getByText(/outdated/i)).toBeInTheDocument();
    expect(screen.getByText("Old info.")).toHaveClass("stale");
  });

  it("renders campaign complete state", () => {
    render(
      <FilmstripBar
        model={model({ route_complete: true, zone_name: "Campaign complete" })}
        zoom={false}
        setupMode={false}
      />,
    );
    expect(screen.getByText(/campaign complete/i)).toBeInTheDocument();
  });

  it("applies zoom and setup-mode classes", () => {
    const { container } = render(
      <FilmstripBar model={model()} zoom={true} setupMode={true} />,
    );
    expect(container.firstChild).toHaveClass("zoom");
    expect(screen.getByText(/drag to move/i)).toBeInTheDocument();
  });

  it("renders the build summary when present, with the build-summary class", () => {
    render(
      <FilmstripBar
        model={model({}, { build_summary: "Ranger (Deadeye) — 12 milestones" })}
        zoom={false}
        setupMode={false}
      />,
    );
    const summary = screen.getByText("Ranger (Deadeye) — 12 milestones");
    expect(summary).toHaveClass("build-summary");
  });

  it("omits the build summary line when there is no build", () => {
    const { container } = render(
      <FilmstripBar model={model()} zoom={false} setupMode={false} />,
    );
    expect(container.querySelector(".build-summary")).not.toBeInTheDocument();
  });

  it("marks the header row as a drag region only in setup mode", () => {
    const { container, rerender } = render(
      <FilmstripBar model={model()} zoom={false} setupMode={true} />,
    );
    const headerRow = container.querySelector(".header-row");
    expect(headerRow).toHaveAttribute("data-tauri-drag-region");

    rerender(<FilmstripBar model={model()} zoom={false} setupMode={false} />);
    expect(container.querySelector(".header-row")).not.toHaveAttribute(
      "data-tauri-drag-region",
    );
  });
});
