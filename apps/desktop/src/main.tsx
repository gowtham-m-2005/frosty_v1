import React from "react";
import ReactDOM from "react-dom/client";

import { MainApp } from "./MainApp";
import { OverlayApp } from "./OverlayApp";
import "./app.css";

// One bundle serves both windows; the overlay is opened with ?window=overlay.
const isOverlay = new URLSearchParams(window.location.search).get("window") === "overlay";
document.documentElement.dataset.window = isOverlay ? "overlay" : "main";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <OverlayApp /> : <MainApp />}</React.StrictMode>,
);
