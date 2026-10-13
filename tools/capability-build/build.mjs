// Builds a capability: src/index.ts(x) -> dist/index.js and, if present,
// src/views.tsx -> dist/views.js.
//
//   node tools/capability-build/build.mjs <capability folder>
//
// `react` and `react/jsx-runtime` are aliased to shims that read the shell's
// React from the global scope, so a capability never ships its own copy
// (docs/PROJECT_SPEC.md 8.10). `@frosty/sdk` is bundled in; it is tiny.
import { createRequire } from "node:module";
import { existsSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const sdk = process.env.FROSTY_SDK ?? path.resolve(here, "../../packages/sdk");
const require = createRequire(path.join(sdk, "package.json"));
const esbuild = require("esbuild");

const dir = path.resolve(process.argv[2] ?? ".");
const first = (...candidates) => candidates.map((c) => path.join(dir, c)).find(existsSync);

const logic = first("src/index.ts", "src/index.tsx", "src/index.js");
if (!logic) {
  console.error(`No src/index.ts in ${dir}`);
  process.exit(1);
}
const views = first("src/views.tsx", "src/views.ts", "src/views.jsx", "src/views.js");

const shared = {
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  jsx: "automatic",
  sourcemap: false,
  logLevel: "info",
  alias: {
    react: path.join(sdk, "src/react-shim.ts"),
    "react/jsx-runtime": path.join(sdk, "src/jsx-runtime-shim.ts"),
    "react/jsx-dev-runtime": path.join(sdk, "src/jsx-runtime-shim.ts"),
    "@frosty/sdk": path.join(sdk, "src/index.ts"),
    "@frosty/sdk/views": path.join(sdk, "src/views.ts"),
  },
};

mkdirSync(path.join(dir, "dist"), { recursive: true });
await esbuild.build({ ...shared, entryPoints: [logic], outfile: path.join(dir, "dist/index.js") });
if (views) {
  await esbuild.build({
    ...shared,
    entryPoints: [views],
    outfile: path.join(dir, "dist/views.js"),
  });
}
console.log(`built ${path.basename(dir)}${views ? " (logic and views)" : " (logic only)"}`);
