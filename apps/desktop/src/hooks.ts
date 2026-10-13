import { useEffect, useRef, useState } from "react";
import { applyTheme } from "@frosty/ui";

import { api, on, type DecisionPayload, type FrostyConfig, type Notice } from "./bridge";

/** Subscribes for the lifetime of the component and cleans up reliably. */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void): void {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void on<T>(event, (payload) => latest.current(payload)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [event]);
}

export function useConfig(): FrostyConfig | null {
  const [config, setConfig] = useState<FrostyConfig | null>(null);
  useEffect(() => {
    void api.getConfig().then(setConfig);
  }, []);
  useTauriEvent<FrostyConfig>("frosty://config", setConfig);
  return config;
}

export function useNotices(): Notice[] {
  const [notices, setNotices] = useState<Notice[]>([]);
  useEffect(() => {
    void api.getNotices().then(setNotices);
  }, []);
  useTauriEvent<Notice[]>("frosty://notices", setNotices);
  return notices;
}

/** Applies the configured theme, mode, and reduced-motion setting to the page. */
export function useTheme(config: FrostyConfig | null) {
  const [fallback, setFallback] = useState<{ theme: string; mode: string } | null>(null);
  const theme = config?.appearance.theme;
  const mode = config?.appearance.mode;
  const motion = config?.character.reduced_motion;

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      void api.resolveTheme(media.matches).then((resolved) => {
        applyTheme(document.documentElement, resolved);
        setFallback(resolved.fallback ? { theme: resolved.name, mode: resolved.mode } : null);
      });
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, mode]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => {
      const reduced = motion === "on" || (motion !== "off" && media.matches);
      document.documentElement.dataset.reducedMotion = String(reduced);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [motion]);

  return { modeFallback: fallback };
}

export interface LoggedDecision extends DecisionPayload {
  at: number;
}

export function useDecisionLog(limit = 40): LoggedDecision[] {
  const [log, setLog] = useState<LoggedDecision[]>([]);
  useTauriEvent<DecisionPayload>("frosty://decision", (payload) => {
    setLog((prev) => [{ ...payload, at: Date.now() }, ...prev].slice(0, limit));
  });
  return log;
}

/** Number of notifications not yet read, kept current. */
export function useUnreadCount(): number {
  const [count, setCount] = useState(0);
  useEffect(() => {
    void api
      .listNotifications(200)
      .then((all) =>
        setCount(
          all.filter(
            (n) =>
              n.read_at === null &&
              n.state !== "dropped_muted" &&
              n.state !== "dropped_rate_limited",
          ).length,
        ),
      );
  }, []);
  useTauriEvent<{ unread: number }>("frosty://notifications-changed", (m) => setCount(m.unread));
  return count;
}
