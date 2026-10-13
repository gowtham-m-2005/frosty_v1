import { LogicHost } from "@frosty/sdk/host";

import { bridge } from "./globals";

let host: LogicHost | null = null;

/**
 * Starts the hidden logic host once. Capability code runs here, in its own
 * webview, whether or not any window is visible.
 */
export function startLogicHost(): void {
  if (host) return;
  host = new LogicHost(bridge, (url) => import(/* @vite-ignore */ url));
  void host.start().catch((e) => console.error("logic host failed to start", e));
}
