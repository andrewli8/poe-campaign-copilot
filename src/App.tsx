import { FilmstripBar } from "./FilmstripBar";
import { useOverlay } from "./useOverlay";
import { useOverlayHeight } from "./useOverlayHeight";

export default function App() {
  const { model, zoom, setupMode, compact, overlayOpacity, runTimer, showRunTimer, nowMs } =
    useOverlay();
  // Callback ref that resizes the overlay window to its rendered height.
  // `model` is null on the first commit (it loads asynchronously), so the
  // root div does not exist yet; a callback ref attaches the ResizeObserver
  // the moment the div mounts, which a ref-object effect would miss. See
  // useOverlayHeight.
  const setRoot = useOverlayHeight();
  if (!model) return null;
  // Opacity is applied here via CSS on the overlay's root wrapper (rather
  // than a native window-opacity API, which Tauri v2 does not expose
  // cross-platform): the window itself is already transparent, so fading
  // the webview content is equivalent and portable. The value is clamped
  // to a 20% floor (frontend and backend) so the overlay can never be
  // faded into unfindability.
  return (
    <div ref={setRoot} style={{ opacity: overlayOpacity }}>
      <FilmstripBar
        model={model}
        zoom={zoom}
        setupMode={setupMode}
        compact={compact}
        runTimer={runTimer}
        showRunTimer={showRunTimer}
        nowMs={nowMs}
      />
    </div>
  );
}
