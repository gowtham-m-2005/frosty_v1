import { describe, expect, it } from "vitest";

import { orderedEntries, widgetFor } from "./SettingsForm";

describe("widgetFor", () => {
  it("infers a control from the schema type", () => {
    expect(widgetFor({ type: "boolean" })).toBe("toggle");
    expect(widgetFor({ type: "integer" })).toBe("number");
    expect(widgetFor({ type: "number" })).toBe("number");
    expect(widgetFor({ type: "string" })).toBe("text");
    expect(widgetFor({ type: "string", enum: ["a", "b"] })).toBe("select");
    expect(widgetFor({ type: "string", format: "time" })).toBe("time");
    expect(widgetFor({ type: "string", format: "date" })).toBe("date");
    expect(widgetFor({ type: "string", format: "color" })).toBe("color");
    expect(widgetFor({ type: "array", items: { type: "string" } })).toBe("list");
    expect(widgetFor({ type: "array", items: { type: "string", enum: ["a"] } })).toBe(
      "multi-select",
    );
  });

  it("honours a valid x-frosty-widget hint and ignores an unknown one", () => {
    expect(widgetFor({ type: "string", "x-frosty-widget": "path" })).toBe("path");
    expect(widgetFor({ type: "string", "x-frosty-widget": "keybinding" })).toBe("keybinding");
    expect(widgetFor({ type: "string", "x-frosty-widget": "hologram" })).toBe("text");
  });
});

describe("orderedEntries", () => {
  it("keeps declaration order unless x-frosty-order says otherwise", () => {
    const entries = orderedEntries({
      properties: {
        b: { type: "string" },
        a: { type: "string", "x-frosty-order": -1 },
        c: { type: "string", "x-frosty-group": "Advanced" },
      },
    });
    expect(entries.map((e) => e.key)).toEqual(["a", "b", "c"]);
    expect(entries[2]?.group).toBe("Advanced");
  });

  it("handles a schema with no properties", () => {
    expect(orderedEntries({})).toEqual([]);
  });
});
