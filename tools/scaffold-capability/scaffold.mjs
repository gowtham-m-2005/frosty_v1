// Generates a new capability folder and builds it.
//
//   node tools/scaffold-capability/scaffold.mjs <id> [--name "Display Name"] [--out <folder>]
//
// With no --out the module goes into your Frosty Home capabilities folder
// (%APPDATA%\frosty\capabilities), so after it builds you can press Rescan in
// the module manager and turn it on. Pass --out capabilities to make it a
// built-in of this repo instead.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const sdk = path.join(repo, "packages/sdk");

const args = process.argv.slice(2);
const id = args.find(
  (a) =>
    !a.startsWith("--") &&
    args[args.indexOf(a) - 1] !== "--name" &&
    args[args.indexOf(a) - 1] !== "--out",
);
const option = (flag) => {
  const i = args.indexOf(flag);
  return i === -1 ? undefined : args[i + 1];
};

if (!id || !/^[a-z][a-z0-9_]{0,31}$/.test(id)) {
  console.error(
    'Usage: node tools/scaffold-capability/scaffold.mjs <id> [--name "Display Name"] [--out <folder>]\n' +
      "The id uses lowercase letters, digits and _, starts with a letter, and is at most 32 characters.",
  );
  process.exit(1);
}

const name = option("--name") ?? id.charAt(0).toUpperCase() + id.slice(1).replace(/_/g, " ");
const outRoot =
  option("--out") ??
  path.join(
    process.env.APPDATA ?? path.join(process.env.HOME ?? ".", ".config"),
    "frosty",
    "capabilities",
  );
const dir = path.resolve(outRoot, id);

if (existsSync(dir)) {
  console.error(`${dir} already exists. Choose another id or remove it first.`);
  process.exit(1);
}

const files = {
  "capability.toml": `[capability]
id = "${id}"
name = "${name}"
version = "0.1.0"
api_version = "1"
runtime = "ts"
description = "Describe what ${name} does in one sentence."
entry = "dist/index.js"
views = "dist/views.js"
icon = "assets/icon.svg"

[permissions]
storage = true

[storage]
migrations = "migrations/"

[settings]
schema = "settings.schema.json"

[[tools]]
id = "${id}.add"
title = "Add an item"
input_schema = "schemas/add.input.json"
quick_action = true
quick_action_control = "text"

[[tools]]
id = "${id}.list"
title = "List items"

[[events]]
id = "${id}.created"

[[reactions]]
on = "${id}.created"
animation = "happy_small"
intensity = "small"
microcopy = "${id}.created"
`,
  "settings.schema.json": `{
  "type": "object",
  "properties": {
    "show_count": {
      "type": "boolean",
      "title": "Show the item count on the dock",
      "default": true
    }
  },
  "additionalProperties": false
}
`,
  "schemas/add.input.json": `{
  "type": "object",
  "required": ["title"],
  "properties": {
    "title": { "type": "string", "minLength": 1, "maxLength": 200 }
  },
  "additionalProperties": false
}
`,
  "migrations/0001_init.sql": `-- Tables are named cap_${id}_<name> and carry the sync columns.
CREATE TABLE cap_${id}_items (
    id         TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER,
    device_id  TEXT NOT NULL,
    title      TEXT NOT NULL
);
`,
  "microcopy/en.json": `{
  "casual": { "${id}.created": ["got it.", "added."] },
  "neutral": { "${id}.created": ["Item added."] }
}
`,
  "assets/icon.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8 12h8M12 8v8"/></svg>
`,
  "src/index.ts": `import { defineCapability } from "@frosty/sdk";

export default defineCapability({
  id: "${id}",

  async onActivate(ctx) {
    const refreshBadge = async () => {
      const settings = await ctx.config.get();
      const rows = await ctx.db.query("items");
      await ctx.views.setBadge(settings.show_count === false ? 0 : rows.length);
    };

    await ctx.tools.register("${id}.add", async ({ title }: { title: string }) => {
      const row = await ctx.db.insert("items", { title });
      await ctx.events.emit("${id}.created", { id: row.id });
      await refreshBadge();
      return { id: row.id };
    });

    await ctx.tools.register("${id}.list", () =>
      ctx.db.query("items", { orderBy: "created_at", descending: true }),
    );

    await ctx.config.onChange(() => void refreshBadge());
    await refreshBadge();
  },

  async onDeactivate(ctx) {
    await ctx.views.setBadge(0);
  },
});
`,
  "src/views.tsx": `import { useState } from "react";
import { defineViews } from "@frosty/sdk";
import { useTool, useToolData } from "@frosty/sdk/views";

function Items() {
  const [title, setTitle] = useState("");
  const add = useTool<{ title: string }>("${id}.add");
  const { data, loading, error, refresh } = useToolData<{ id: string; title: string }[]>(
    "${id}.list",
    {},
    ["${id}.created"],
  );

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <form
        style={{ display: "flex", gap: 8 }}
        onSubmit={(e) => {
          e.preventDefault();
          if (title.trim()) void add({ title: title.trim() }).then(() => (setTitle(""), refresh()));
        }}
      >
        <input
          className="f-input"
          aria-label="New item"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          style={{ flex: 1 }}
        />
        <button type="submit" className="f-button f-button-primary">Add</button>
      </form>
      {error ? <p role="alert">{error}</p> : null}
      {loading && !data ? <p className="muted">Loading…</p> : null}
      {data && data.length === 0 ? <p className="muted">Nothing here yet.</p> : null}
      <ul>
        {(data ?? []).map((item) => (
          <li key={item.id}>{item.title}</li>
        ))}
      </ul>
    </div>
  );
}

export default defineViews({
  card: () => <Items />,
  page: () => <Items />,
});
`,
  "tsconfig.json": `{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "baseUrl": ".",
    "paths": {
      "@frosty/sdk": ["${path.join(sdk, "src/index.ts").replaceAll("\\", "/")}"],
      "@frosty/sdk/views": ["${path.join(sdk, "src/views.ts").replaceAll("\\", "/")}"]
    }
  },
  "include": ["src"]
}
`,
};

for (const [rel, content] of Object.entries(files)) {
  const target = path.join(dir, rel);
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, content);
}
console.log(`created ${dir}`);

execFileSync(process.execPath, [path.join(repo, "tools/capability-build/build.mjs"), dir], {
  stdio: "inherit",
  env: { ...process.env, FROSTY_SDK: sdk },
});
console.log(`\nNext: open Frosty, go to Modules, press Rescan, and turn ${name} on.`);
