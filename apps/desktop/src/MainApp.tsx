import { useCallback, useEffect, useState } from "react";
import {
  FOX_SIZES,
  FoxView,
  STATE_DURATION_MS,
  isFoxState,
  type FoxState,
} from "@frosty/character";
import { Banner, Button } from "@frosty/ui";

import { api, type DecisionPayload } from "./bridge";
import { useConfig, useNotices, useTauriEvent, useTheme } from "./hooks";
import { DebugPage } from "./pages/DebugPage";
import { SettingsPage } from "./pages/SettingsPage";

type Page = "home" | "modules" | "settings" | "debug";

const PAGES: { id: Page; label: string; dev?: boolean }[] = [
  { id: "home", label: "Home" },
  { id: "modules", label: "Modules" },
  { id: "settings", label: "Settings" },
  { id: "debug", label: "Debug", dev: true },
];

/** Fullscreen mode: rail, top bar, and the pages. */
export function MainApp() {
  const config = useConfig();
  const { modeFallback } = useTheme(config);
  const notices = useNotices();
  const [page, setPage] = useState<Page>("home");
  const [foxState, setFoxState] = useState<FoxState>("idle");
  const [foxIntensity, setFoxIntensity] = useState(0.5);
  const [playKey, setPlayKey] = useState(0);

  const play = useCallback((state: FoxState, amount: number) => {
    setFoxState(state);
    setFoxIntensity(amount);
    setPlayKey((k) => k + 1);
    const duration = STATE_DURATION_MS[state];
    if (duration !== null) {
      window.setTimeout(() => setFoxState("idle"), duration);
    }
  }, []);

  useTauriEvent<DecisionPayload>("frosty://decision", ({ decision }) => {
    if (decision.kind === "play" && decision.animation && isFoxState(decision.animation)) {
      play(decision.animation, decision.intensity);
    }
  });

  useTauriEvent<string>("frosty://navigate", (target) => {
    if (PAGES.some((p) => p.id === target)) setPage(target as Page);
  });

  // An empty home is a moment for the curious tilt (docs/DESIGN_SPEC.md 14).
  useEffect(() => {
    if (page === "home") void api.fireEvent("core.empty_state");
  }, [page]);

  const visiblePages = PAGES.filter((p) => !p.dev || import.meta.env.DEV);
  const title = PAGES.find((p) => p.id === page)?.label ?? "";

  return (
    <div className="main-shell">
      <nav className="rail" aria-label="Main navigation">
        <div className="rail-fox">
          <FoxView
            state={foxState}
            intensity={foxIntensity}
            size={FOX_SIZES.small / 2}
            playKey={playKey}
          />
        </div>
        {visiblePages.map((p) => (
          <button
            key={p.id}
            type="button"
            className="rail-button"
            aria-current={page === p.id ? "page" : undefined}
            onClick={() => setPage(p.id)}
          >
            {p.label}
          </button>
        ))}
      </nav>

      <div className="main-column">
        <header className="topbar">
          <h1>{title}</h1>
          <Button onClick={() => void api.toggleFullscreen()}>Back to compact</Button>
        </header>

        <main className="content">
          {notices.map((n) => (
            <Banner key={`${n.source}-${n.message}`} level={n.level}>
              {n.message}
            </Banner>
          ))}
          {modeFallback ? (
            <Banner level="info">
              {modeFallback.theme} does not have the mode you chose, so it is showing in{" "}
              {modeFallback.mode} mode.
            </Banner>
          ) : null}

          {page === "home" && (
            <section className="empty-state">
              <h2>Nothing here yet</h2>
              <p>Modules add abilities to Frosty. Pick one to get started.</p>
              <Button variant="primary" onClick={() => setPage("modules")}>
                Add your first module
              </Button>
            </section>
          )}
          {page === "modules" && (
            <section className="empty-state">
              <h2>No modules installed</h2>
              <p>The module manager arrives with the capability system in the next phase.</p>
            </section>
          )}
          {page === "settings" && config && <SettingsPage config={config} />}
          {page === "debug" && <DebugPage />}
        </main>
      </div>
    </div>
  );
}
