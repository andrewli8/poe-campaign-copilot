import "./FilmstripBar.css";
import {
  elapsedMs,
  formatElapsed,
  isRunning,
  type RunTimerState,
} from "./runTimer";
import type { NoteCategory, UiModel } from "./types";

// Per-category glyph + label for the note cards (see FilmstripBar.css for
// the matching colours). Objective = "do this" (gold), Layout = where to go
// (teal), Danger = a warning like capping resistances (coral).
const NOTE_ICON: Record<NoteCategory, string> = {
  objective: "⚑", // ⚑
  layout: "◇", // ◇
  danger: "△", // △
};
const NOTE_LABEL: Record<NoteCategory, string> = {
  objective: "Objective",
  layout: "Layout",
  danger: "Danger",
};

export interface FilmstripBarProps {
  model: UiModel;
  zoom: boolean;
  setupMode: boolean;
  compact: boolean;
  runTimer: RunTimerState;
  showRunTimer: boolean;
  nowMs: number;
}

export function FilmstripBar({
  model,
  zoom,
  setupMode,
  compact,
  runTimer,
  showRunTimer,
  nowMs,
}: FilmstripBarProps) {
  const rootClass = [
    "filmstrip",
    zoom && "zoom",
    setupMode && "setup-mode",
    compact && "compact",
  ]
    .filter(Boolean)
    .join(" ");

  // Paused means "has run, currently stopped" — never-started renders
  // without the pause cue (0:00:00, waiting for its first zone entry).
  const timerPaused = !isRunning(runTimer) && runTimer.accumulated_ms > 0;
  const timerChip = showRunTimer ? (
    <span className={["run-timer", timerPaused && "paused"].filter(Boolean).join(" ")}>
      {timerPaused && <span className="run-timer-pause-icon">⏸</span>}
      {formatElapsed(elapsedMs(runTimer, nowMs))}
    </span>
  ) : null;

  if (model.waiting_for_log) {
    return (
      <div className={rootClass}>
        {setupMode && (
          <div className="setup-hint">drag to move &middot; resize edges &middot; toggle via tray</div>
        )}
        <div className="waiting-pill">Waiting for Client.txt&hellip;</div>
        {setupMode && <div className="drag-layer" data-tauri-drag-region aria-hidden="true" />}
      </div>
    );
  }

  const { overlay, images } = model;

  const locationChip =
    overlay.location_status !== "on_track" ? (
      <span
        className={[
          "location-chip",
          overlay.location_status === "catching_up" ? "catching-up" : "revisiting",
        ].join(" ")}
      >
        {overlay.location_status === "catching_up" ? "Catching up" : "Revisiting"}
      </span>
    ) : null;

  return (
    // Tauri v2 only starts a window drag when the mousedown TARGET element
    // itself carries data-tauri-drag-region — a click that lands on a child
    // (zone-name span, image, list item, etc.) does NOT drag, even if that
    // child is nested under an element with the attribute. Putting the
    // attribute on the root therefore only made the bare gaps between
    // children draggable. Instead we render a dedicated, fully transparent
    // drag-layer as the LAST child, absolutely positioned over the whole
    // bar (see .drag-layer in FilmstripBar.css). It has no interactive
    // content, so every click in setup mode lands on it and drags — the bar
    // is grabbable from anywhere, not just a header strip or empty gaps.
    <div className={rootClass}>
      {setupMode && (
        <div className="setup-hint">drag to move &middot; resize edges &middot; toggle via tray</div>
      )}

      {/* Compact only strips the normal-playing layout; the build summary
          still shows alongside the complete-bar/waiting-pill status lines. */}
      {(!compact || overlay.route_complete) && model.build_summary && (
        <div className="build-summary">{model.build_summary}</div>
      )}

      {overlay.route_complete ? (
        <div className="complete-bar">Campaign complete</div>
      ) : compact ? (
        <div className="compact-row">
          <span className="zone-name">{overlay.zone_name}</span>
          {locationChip}
          <span className="compact-primary">{overlay.primary}</span>
          {overlay.next_zone && (
            <span className="compact-next">&rarr; {overlay.next_zone}</span>
          )}
          {timerChip}
        </div>
      ) : (
        <>
          <div className="header-row">
            <span className="zone-name">{overlay.zone_name}</span>
            <span className="act-badge">ACT {overlay.act}</span>
            {/* Intentionally overlay.layout_images.length, not images.length below —
                overlay.layout_images (composer) and images (pipeline-encoded, data-url-bearing)
                are separate Rust-side lists that happen to be parallel in practice. */}
            <span className="layout-count">{overlay.layout_images.length} images</span>
            {overlay.pending_count > 0 && (
              <span className="pending-badge">&#9675; {overlay.pending_count} pending</span>
            )}
            {overlay.is_town && <span className="town-chip">TOWN</span>}
            {locationChip}
            {timerChip}
          </div>

          {overlay.off_route_zone && (
            <div className="off-route-banner">
              In {overlay.off_route_zone} &mdash; off route
            </div>
          )}

          {overlay.groups_behind > 0 && (
            <div className="breadcrumb-line">
              {overlay.groups_behind} zone{overlay.groups_behind !== 1 ? "s" : ""} behind your
              furthest point
            </div>
          )}

          {images.length > 0 && (
            <div className="image-row">
              {images.map((img) => (
                <div key={img.file} className={["image-cell", img.stale && "stale"].filter(Boolean).join(" ")}>
                  <img src={img.data_url} alt={img.file} className={img.stale ? "stale" : undefined} />
                  {img.stale && <span className="outdated-badge">outdated</span>}
                </div>
              ))}
            </div>
          )}

          {/* Objective card — the current route step, styled as the
              gold "do this" card. Zone sub-steps and directional hints sit
              beneath it. */}
          {overlay.primary && (
            <div className="note-card objective">
              <span className="nc-ic" aria-hidden="true">{NOTE_ICON.objective}</span>
              <div className="nc-body">
                <div className="nc-kicker">{NOTE_LABEL.objective}</div>
                <div className="nc-text">{overlay.primary}</div>
                {overlay.steps_in_zone.length > 1 && (
                  <ul className="nc-sub">
                    {overlay.steps_in_zone.slice(1).map((step, i) => (
                      <li key={i}>{step}</li>
                    ))}
                  </ul>
                )}
                {overlay.sub_hints.map((hint, i) => (
                  <div key={i} className="nc-hint">
                    {hint}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Layout / danger note cards — colour-coded by category.
              Outdated notes are dropped upstream, so every note shown is
              current (no strike-through). */}
          {overlay.layout_notes.map((note, i) => (
            <div key={i} className={`note-card ${note.category}`}>
              <span className="nc-ic" aria-hidden="true">{NOTE_ICON[note.category]}</span>
              <div className="nc-body">
                <div className="nc-kicker">{NOTE_LABEL[note.category]}</div>
                <div className="nc-text">{note.text}</div>
              </div>
            </div>
          ))}

          {/* Next zone — the single most navigational thing, a gold bar. */}
          {overlay.next_zone && (
            <div className="next-bar">
              <span className="nb-label">Next</span>
              <span className="nb-dest">{overlay.next_zone}</span>
              <span className="nb-chev" aria-hidden="true">&rarr;</span>
            </div>
          )}

          {overlay.town_reminders.length > 0 && (
            <ul className="town-reminders">
              {overlay.town_reminders.map((reminder, i) => (
                <li key={i}>{reminder}</li>
              ))}
            </ul>
          )}
          {overlay.build_reminders.length > 0 && (
            <ul className="build-reminders">
              {overlay.build_reminders.map((reminder, i) => (
                <li key={i} className="build">
                  {reminder}
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      {setupMode && <div className="drag-layer" data-tauri-drag-region aria-hidden="true" />}
    </div>
  );
}
