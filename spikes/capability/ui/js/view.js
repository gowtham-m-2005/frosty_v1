// View window: renders a capability's React component with the shell's React
// and calls its tool through the Rust core.
import React from "react";
import { createRoot } from "react-dom/client";

const { invoke } = window.__TAURI__.core;
const report = (line) => invoke("report", { line });
const h = React.createElement;

const BASE = "http://frosty-cap.localhost";
const views = await import(`${BASE}/hello/dist/views.js`);

function Shell() {
  const [reply, setReply] = React.useState("none yet");
  return h(
    "div",
    null,
    h(views.Card, {
      onCall: async () => {
        const r = await invoke("call_tool", { tool: "hello.greet", input: { name: "view" } });
        setReply(JSON.stringify(r));
      },
    }),
    h("p", { id: "reply" }, reply),
  );
}

createRoot(document.getElementById("root")).render(h(Shell));

// Self-test: click the capability's button, which uses a hook from "react".
await new Promise((r) => setTimeout(r, 500));
const button = document.getElementById("card-button");
if (!button) {
  await report("view: FAIL, capability component did not render");
} else {
  button.click();
  await new Promise((r) => setTimeout(r, 800));
  const clicked = document.getElementById("card-button").textContent;
  const reply = document.getElementById("reply").textContent;
  await report(
    clicked.includes("1")
      ? `view: PASS, capability component rendered by the shell's React and its useState worked ("${clicked}")`
      : `view: FAIL, hook did not update ("${clicked}")`,
  );
  await report(
    reply.includes("hello view")
      ? `view: PASS, tool call through the core returned ${reply}`
      : `view: FAIL, tool reply was ${reply}`,
  );
}
