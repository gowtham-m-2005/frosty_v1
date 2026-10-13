import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Bridge } from "@frosty/sdk";
import type { FrostyGlobals } from "@frosty/sdk/views";

export const bridge: Bridge = {
  invoke: (command, args) => invoke(command, args),
  listen: (event, handler) => listen(event, (e) => handler(e.payload as never)),
};

/** The id of the capability whose view is rendering. */
export const CapabilityContext = React.createContext<string | null>(null);

/**
 * Capability bundles do not ship their own React or bridge. They read these
 * from the global scope, so there is exactly one React instance
 * (docs/PROJECT_SPEC.md 8.10). Must run before any capability module loads.
 */
export function installFrostyGlobals(): void {
  const globals: FrostyGlobals = { React, bridge, CapabilityContext };
  (globalThis as { __frosty?: FrostyGlobals }).__frosty = globals;
}
