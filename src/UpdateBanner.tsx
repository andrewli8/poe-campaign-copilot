import "./UpdateBanner.css";
import type { UpdaterStatus } from "./useUpdater";

export interface UpdateBannerProps {
  status: UpdaterStatus;
  version: string | null;
  progressPct: number | null;
  error: string | null;
  onUpdate: () => void;
}

/// PURE presentational banner shown at the top of the settings window.
/// `checking` / `none` / `idle` render nothing so the settings form has no
/// visible update chrome unless there's actually something to say; `error`
/// is a small, non-blocking line (Settings stays fully usable) rather than
/// anything that could block the rest of the page.
export function UpdateBanner({ status, version, progressPct, error, onUpdate }: UpdateBannerProps) {
  if (status === "available") {
    return (
      <div className="update-banner update-banner-available">
        <span className="update-banner-message">Version {version} is available.</span>
        <button type="button" className="btn btn-primary" onClick={onUpdate}>
          Update and restart
        </button>
      </div>
    );
  }

  if (status === "downloading") {
    return (
      <div className="update-banner update-banner-downloading">
        <span className="update-banner-message">
          Downloading&hellip; {progressPct !== null ? `${progressPct}%` : ""}
        </span>
        <button type="button" className="btn btn-primary" disabled>
          Update and restart
        </button>
      </div>
    );
  }

  if (status === "error") {
    return <div className="update-banner-error">Update check failed{error ? `: ${error}` : ""}</div>;
  }

  return null;
}
