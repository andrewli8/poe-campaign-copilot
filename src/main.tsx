import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsContainer } from "./SettingsContainer";

// The settings window and the main overlay window share this same
// index.html entry point (Tauri's WebviewUrl points both at "index.html",
// the settings one with a "?window=settings" suffix — see
// open_settings_window in src-tauri/src/main.rs) so routing between them
// happens here, client-side, on the URL rather than via two separate HTML
// files.
const isSettingsWindow = new URLSearchParams(location.search).get("window") === "settings";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isSettingsWindow ? <SettingsContainer /> : <App />}
  </React.StrictMode>,
);
