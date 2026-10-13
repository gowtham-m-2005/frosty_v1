// Logic host: loads capability logic from outside the bundle and serves tool
// calls routed through the Rust core. No UI here.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const report = (line) => invoke("report", { line });

const BASE = "http://frosty-cap.localhost";
const tools = new Map();
let ticks = 0;
setInterval(() => {
  ticks += 1;
}, 1000);

const ctx = {
  tools: {
    register(id, handler) {
      tools.set(id, handler);
    },
  },
};

try {
  const mod = await import(`${BASE}/hello/dist/index.js`);
  await mod.default.onActivate(ctx);
  await report(`logic: loaded ${mod.default.id} from an external folder over frosty-cap and activated, tools: ${[...tools.keys()].join(", ")}`);
} catch (e) {
  await report(`logic: FAIL loading capability: ${e}`);
}

// A capability must not be able to reach the network on its own.
try {
  await fetch("https://example.com/", { mode: "no-cors" });
  await report("csp: FAIL, direct fetch to example.com was allowed");
} catch (e) {
  await report(`csp: PASS, direct network fetch from the logic host was blocked (${e.message})`);
}

// A capability must not be able to load code from outside the allowed origins.
try {
  await import("https://example.com/x.js");
  await report("csp: FAIL, import from an arbitrary origin was allowed");
} catch {
  await report("csp: PASS, import from an arbitrary origin was blocked");
}

await listen("tool-call", async ({ payload }) => {
  const { id, tool, input } = payload;
  const handler = tools.get(tool);
  if (!handler) {
    await invoke("tool_result", { id, result: { ok: false, error: `no such tool ${tool}`, ticks } });
    return;
  }
  try {
    const value = await handler(input);
    await invoke("tool_result", { id, result: { ok: true, value, ticks } });
  } catch (e) {
    await invoke("tool_result", { id, result: { ok: false, error: String(e), ticks } });
  }
});

await invoke("logic_ready");
