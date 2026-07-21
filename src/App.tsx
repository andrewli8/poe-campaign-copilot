import { FilmstripBar } from "./FilmstripBar";
import { useOverlay } from "./useOverlay";

export default function App() {
  const { model, zoom, setupMode } = useOverlay();
  if (!model) return null;
  return <FilmstripBar model={model} zoom={zoom} setupMode={setupMode} />;
}
