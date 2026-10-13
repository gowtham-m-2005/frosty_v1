import { Component, useEffect, useState, type ReactNode } from "react";
import type { ViewsDefinition } from "@frosty/sdk";
import { ErrorPanel } from "@frosty/ui";

import { api, type CapabilityInfo, type CapabilityList } from "./bridge";
import { CapabilityContext } from "./globals";
import { useTauriEvent } from "./hooks";

export function useCapabilities(): CapabilityList {
  const [list, setList] = useState<CapabilityList>({ capabilities: [], rejected: [] });
  useEffect(() => {
    void api.listCapabilities().then(setList);
  }, []);
  useTauriEvent<CapabilityList>("frosty://capabilities", setList);
  return list;
}

const viewsCache = new Map<string, Promise<ViewsDefinition>>();

/** Loads a capability's views module once per URL. */
export function loadViews(info: CapabilityInfo): Promise<ViewsDefinition> {
  const url = info.views_url;
  if (!url) return Promise.resolve({});
  let cached = viewsCache.get(url);
  if (!cached) {
    cached = import(/* @vite-ignore */ url).then(
      (mod: { default?: ViewsDefinition }) => mod.default ?? {},
    );
    // A failed load must be retryable after the user fixes and rebuilds it.
    cached.catch(() => viewsCache.delete(url));
    viewsCache.set(url, cached);
  }
  return cached;
}

function copy(text: string) {
  void navigator.clipboard?.writeText(text);
}

interface BoundaryProps {
  info: CapabilityInfo;
  children: ReactNode;
}

/** Contains a crashing view so the shell keeps running (docs/DESIGN_SPEC.md 14). */
class Boundary extends Component<BoundaryProps, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    void api.capabilityFailed(this.props.info.id, `view crashed: ${error.message}`);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const { info } = this.props;
    return (
      <ErrorPanel
        title={`${info.name} stopped working`}
        message={`${error.message}. Frosty is still running; only this module is affected.`}
        onCopy={() => copy(`${info.id} ${info.version}\n${error.stack ?? error.message}`)}
        onDisable={() => void api.disableCapability(info.id)}
      />
    );
  }
}

type Slot = "card" | "page" | "widget";

/** Renders one slot of a capability inside its error boundary. */
export function CapabilitySlot({ info, slot }: { info: CapabilityInfo; slot: Slot }) {
  const [views, setViews] = useState<ViewsDefinition | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadViews(info).then(
      (v) => !cancelled && setViews(v),
      (e: unknown) => !cancelled && setLoadError(e instanceof Error ? e.message : String(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [info]);

  if (loadError) {
    return (
      <ErrorPanel
        title={`${info.name} could not load its views`}
        message={loadError}
        onCopy={() => copy(loadError)}
        onDisable={() => void api.disableCapability(info.id)}
      />
    );
  }
  if (!views) return <p className="muted">Loading…</p>;

  // A module without a `page` falls back to its card (docs/PROJECT_SPEC.md 8.4).
  const Slot = views[slot] ?? (slot === "page" ? views.card : undefined);
  if (!Slot) return <p className="muted">{info.name} has nothing to show here.</p>;

  return (
    <CapabilityContext.Provider value={info.id}>
      <Boundary info={info}>
        <Slot slot={slot} />
      </Boundary>
    </CapabilityContext.Provider>
  );
}
