// Builds the two shared modules the import map points at. react-dom marks
// "react" external, so both resolve to the one vendor-react.js instance.
import { build } from "esbuild";
import { mkdirSync, writeFileSync } from "node:fs";

mkdirSync("tmp", { recursive: true });
writeFileSync(
  "tmp/entry-react.js",
  `import React from "react";
export default React;
export const {
  Children, Fragment, StrictMode, Suspense, createContext, createElement, forwardRef,
  isValidElement, lazy, memo, startTransition, useCallback, useContext, useDeferredValue,
  useEffect, useId, useLayoutEffect, useMemo, useReducer, useRef, useState, useTransition,
} = React;
`,
);
writeFileSync(
  "tmp/entry-react-dom-client.js",
  `import { createRoot, hydrateRoot } from "react-dom/client";
export { createRoot, hydrateRoot };
`,
);

const common = { bundle: true, format: "esm", platform: "browser", minify: true, logLevel: "info" };
await build({
  ...common,
  entryPoints: ["tmp/entry-react.js"],
  outfile: "../ui/js/vendor-react.js",
  define: { "process.env.NODE_ENV": '"production"' },
});
await build({
  ...common,
  entryPoints: ["tmp/entry-react-dom-client.js"],
  outfile: "../ui/js/vendor-react-dom-client.js",
  external: ["react"],
  define: { "process.env.NODE_ENV": '"production"' },
});
