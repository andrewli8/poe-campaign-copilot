import "./FilmstripBar.css";
import type { UiModel } from "./types";

export interface FilmstripBarProps {
  model: UiModel;
  zoom: boolean;
  setupMode: boolean;
}

export function FilmstripBar({ model, zoom, setupMode }: FilmstripBarProps) {
  const rootClass = [
    "filmstrip",
    zoom && "zoom",
    setupMode && "setup-mode",
  ]
    .filter(Boolean)
    .join(" ");

  if (model.waiting_for_log) {
    return (
      <div className={rootClass} data-tauri-drag-region={setupMode ? true : undefined}>
        {setupMode && (
          <div className="setup-hint">drag to move &middot; resize edges &middot; toggle via tray</div>
        )}
        <div className="waiting-pill">Waiting for Client.txt&hellip;</div>
      </div>
    );
  }

  const { overlay, images } = model;

  return (
    // In setup mode the WHOLE bar is a drag region, not just the header row.
    // The overlay contains no interactive elements (no buttons/inputs/links),
    // so Tauri's drag region is never swallowed by a child, and covering the
    // full bar means you can grab it anywhere to move it — important once the
    // bar has been resized larger, when a header-only strip is a tiny target.
    // The OS still owns the few-pixel resize border at the window edges.
    <div className={rootClass} data-tauri-drag-region={setupMode ? true : undefined}>
      {setupMode && (
        <div className="setup-hint">drag to move &middot; resize edges &middot; toggle via tray</div>
      )}

      {model.build_summary && (
        <div className="build-summary">{model.build_summary}</div>
      )}

      {overlay.route_complete ? (
        <div className="complete-bar">Campaign complete</div>
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
          </div>

          {overlay.off_route_zone && (
            <div className="off-route-banner">
              In {overlay.off_route_zone} &mdash; off route
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

          {overlay.layout_notes.length > 0 && (
            <ul className="notes-list">
              {overlay.layout_notes.map((note, i) => (
                <li key={i} className={note.stale ? "stale" : undefined}>
                  {note.text}
                </li>
              ))}
            </ul>
          )}

          <div className="text-block">
            <div className="primary">{overlay.primary}</div>
            {overlay.steps_in_zone.length > 1 && (
              <ul className="steps-list">
                {overlay.steps_in_zone.slice(1).map((step, i) => (
                  <li key={i}>{step}</li>
                ))}
              </ul>
            )}
            {overlay.sub_hints.length > 0 && (
              <div className="sub-hints">
                {overlay.sub_hints.map((hint, i) => (
                  <div key={i} className="sub-hint">
                    {hint}
                  </div>
                ))}
              </div>
            )}
            {overlay.next_zone && <div className="next-line">Next: {overlay.next_zone}</div>}
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
          </div>
        </>
      )}
    </div>
  );
}
