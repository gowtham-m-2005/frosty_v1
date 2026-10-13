import { useCallback, useEffect, useMemo, useState } from "react";
import {
  FOX_SIZES,
  FoxView,
  STATE_DURATION_MS,
  isFoxState,
  type FoxState,
} from "@frosty/character";
import { Banner, Button } from "@frosty/ui";

import { api, type DecisionPayload } from "./bridge";
import { CapabilitySlot, useCapabilities } from "./capabilities";
import { useConfig, useNotices, useTauriEvent, useTheme } from "./hooks";
import { DebugPage } from "./pages/DebugPage";
import { ModulesPage } from "./pages/ModulesPage";
import { SettingsPage } from "./pages/SettingsPage";

/** `home`, `modules`, `settings`, `debug`, or `module:<id>`. */
type Page = string;

const FIXED_PAGES: { id: string; label: string; dev?: boolean }[] = [
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
  const { capabilities } = useCapabilities();
  const [requestedPage, setPage] = useState<Page>("home");
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

  useTauriEvent<string>("frosty://navigate", (target) => setPage(target));

  const modulePages = useMemo(
    () => capabilities.filter((c) => c.state === "enabled" && c.views_url),
    [capabilities],
  );
  const fixed = FIXED_PAGES.filter((p) => !p.dev || import.meta.env.DEV);
  // If the module behind the open page is turned off, show Modules instead.
  const page =
    requestedPage.startsWith("module:") &&
    !modulePages.some((c) => `module:${c.id}` === requestedPage)
      ? "modules"
      : requestedPage;
  const activeModule = page.startsWith("module:")
    ? capabilities.find((c) => `module:${c.id}` === page)
    : undefined;

  // An empty home is a moment for the curious tilt (docs/DESIGN_SPEC.md 14).
  useEffect(() => {
    if (page === "home") void api.fireEvent("core.empty_state");
  }, [page]);

  const title = activeModule?.name ?? FIXED_PAGES.find((p) => p.id === page)?.label ?? "";

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
        {fixed.slice(0, 1).map((p) => (
          <RailButton key={p.id} page={page} id={p.id} label={p.label} onSelect={setPage} />
        ))}
        {modulePages.map((c) => (
          <RailButton
            key={c.id}
            page={page}
            id={`module:${c.id}`}
            label={c.name}
            badge={c.badge}
            onSelect={setPage}
          />
        ))}
        {fixed.slice(1).map((p) => (
          <RailButton key={p.id} page={page} id={p.id} label={p.label} onSelect={setPage} />
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
                {modulePages.length === 0 ? "Add your first module" : "Open the module manager"}
              </Button>
            </section>
          )}
          {page === "modules" && <ModulesPage />}
          {page === "settings" && config && <SettingsPage config={config} />}
          {page === "debug" && <DebugPage />}
          {activeModule ? <CapabilitySlot info={activeModule} slot="page" /> : null}
        </main>
      </div>
    </div>
  );
}

function RailButton({
  page,
  id,
  label,
  badge,
  onSelect,
}: {
  page: string;
  id: string;
  label: string;
  badge?: number;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className="rail-button"
      aria-current={page === id ? "page" : undefined}
      onClick={() => onSelect(id)}
    >
      {label}
      {badge ? <span className="rail-badge"> {badge > 9 ? "9+" : badge}</span> : null}
    </button>
  );
}
