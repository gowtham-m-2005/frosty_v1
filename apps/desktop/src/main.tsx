import React from "react";
import ReactDOM from "react-dom/client";

import { installFrostyGlobals } from "./globals";
import { startLogicHost } from "./logicHost";
import { MainApp } from "./MainApp";
import { OverlayApp } from "./OverlayApp";
import "./app.css";

// One bundle serves all three windows. The core opens the overlay and the
// logic host with ?window=overlay and ?window=logic.
const windowName = new URLSearchParams(window.location.search).get("window") ?? "main";
document.documentElement.dataset.window = windowName;

installFrostyGlobals();

if (windowName === "logic") {
  // No UI: this webview only runs capability logic.
  startLogicHost();
} else {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{windowName === "overlay" ? <OverlayApp /> : <MainApp />}</React.StrictMode>,
  );
}
