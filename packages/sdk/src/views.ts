/**
 * Hooks for capability views. A view never gets `ctx`: it reaches its logic
 * only through tools and events (docs/DESIGN_SPEC.md 7.2 rule 10).
 *
 * React and the bridge come from the shell through `globalThis.__frosty`, so
 * a capability bundle shares the shell's single React instance.
 */
/* eslint-disable react-hooks/immutability --
   React comes from the shell's globals rather than an import, so the compiler
   rules cannot see that these are ordinary hooks and refs. */
import type * as ReactTypes from "react";

import type { Bridge, FrostyEvent, Json } from "./types";
import { matchesPattern } from "./types";

export interface FrostyGlobals {
  React: typeof ReactTypes;
  bridge: Bridge;
  /** Provides the id of the capability whose view is rendering. */
  CapabilityContext: ReactTypes.Context<string | null>;
}

function frosty(): FrostyGlobals {
  const g = (globalThis as { __frosty?: FrostyGlobals }).__frosty;
  if (!g) throw new Error("Frosty shell globals are missing; views must run inside Frosty");
  return g;
}

export function useCapabilityId(): string {
  const { React, CapabilityContext } = frosty();
  const id = React.useContext(CapabilityContext);
  if (!id) throw new Error("this view is not inside a Frosty capability");
  return id;
}

/** Returns a function that calls one of this capability's tools. */
export function useTool<
  Input extends Record<string, Json> = Record<string, Json>,
  Output = unknown,
>(tool: string): (input?: Input) => Promise<Output> {
  const { React, bridge } = frosty();
  const id = useCapabilityId();
  return React.useCallback(
    (input?: Input) => bridge.invoke<Output>("call_tool", { caller: id, tool, input: input ?? {} }),
    [bridge, id, tool],
  );
}

/** Calls `handler` for each event matching `pattern`. */
export function useEvent(pattern: string, handler: (event: FrostyEvent) => void): void {
  const { React, bridge } = frosty();
  const latest = React.useRef(handler);
  React.useEffect(() => {
    latest.current = handler;
  });
  React.useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void bridge
      .listen<FrostyEvent>("frosty://event", (event) => {
        if (matchesPattern(pattern, event.name)) latest.current(event);
      })
      .then((fn) => {
        if (cancelled) fn();
        else off = fn;
      });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [bridge, pattern]);
}

export interface ToolData<T> {
  data: T | undefined;
  loading: boolean;
  error: string | undefined;
  refresh: () => void;
}

/**
 * Calls a tool on mount and again whenever one of the `refreshOn` events
 * arrives. Handles the loading and error states every view needs.
 */
export function useToolData<Output = unknown>(
  tool: string,
  input: Record<string, Json> = {},
  refreshOn: string[] = [],
): ToolData<Output> {
  const { React } = frosty();
  const call = useTool<Record<string, Json>, Output>(tool);
  const [state, setState] = React.useState<{
    data: Output | undefined;
    loading: boolean;
    error: string | undefined;
  }>({ data: undefined, loading: true, error: undefined });
  const inputKey = JSON.stringify(input);

  const refresh = React.useCallback(() => {
    setState((s) => ({ ...s, loading: true }));
    call(JSON.parse(inputKey) as Record<string, Json>).then(
      (data) => setState({ data, loading: false, error: undefined }),
      (e: unknown) =>
        setState({
          data: undefined,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }),
    );
  }, [call, inputKey]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const { bridge } = frosty();
  const patterns = refreshOn.join("|");
  React.useEffect(() => {
    if (!patterns) return;
    const list = patterns.split("|");
    let off: (() => void) | undefined;
    let cancelled = false;
    void bridge
      .listen<FrostyEvent>("frosty://event", (event) => {
        if (list.some((p) => matchesPattern(p, event.name))) refresh();
      })
      .then((fn) => {
        if (cancelled) fn();
        else off = fn;
      });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [bridge, patterns, refresh]);

  return { ...state, refresh };
}

/** This capability's settings, updated live when they change. */
export function useSettings<T extends Record<string, unknown> = Record<string, unknown>>():
  T | undefined {
  const { React, bridge } = frosty();
  const id = useCapabilityId();
  const [values, setValues] = React.useState<T | undefined>(undefined);
  React.useEffect(() => {
    let cancelled = false;
    let off: (() => void) | undefined;
    void bridge.invoke<T>("get_capability_settings", { id }).then((v) => {
      if (!cancelled) setValues(v);
    });
    void bridge
      .listen<{ id: string; values: T }>("frosty://capability-config", (m) => {
        if (m.id === id) setValues(m.values);
      })
      .then((fn) => {
        if (cancelled) fn();
        else off = fn;
      });
    return () => {
      cancelled = true;
      off?.();
    };
  }, [bridge, id]);
  return values;
}
