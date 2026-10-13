import { describe, expect, it } from "vitest";

import { themeToCssVars, type ResolvedTheme } from "./theme";

const base: ResolvedTheme = {
  id: "test",
  name: "Test",
  mode: "light",
  fallback: false,
  tokens: { bg: "#ffffff", "text-muted": "#555555" },
  options: {},
};

describe("themeToCssVars", () => {
  it("exposes every token as a custom property", () => {
    const vars = themeToCssVars(base);
    expect(vars["--bg"]).toBe("#ffffff");
    expect(vars["--text-muted"]).toBe("#555555");
  });

  it("uses soft defaults when the theme sets no options", () => {
    const vars = themeToCssVars(base);
    expect(vars["--radius-scale"]).toBe("1");
    expect(vars["--border-width"]).toBe("1px");
    expect(vars["--shadow"]).toBe("0 6px 20px rgba(0, 0, 0, 0.18)");
    expect(vars["--surface-blur"]).toBe("none");
  });

  it("maps pixel theme options", () => {
    const vars = themeToCssVars({
      ...base,
      options: {
        "radius-scale": "0",
        "border-width": "2",
        "shadow-style": "hard",
        "surface-style": "solid",
        "font-display": "Pixelify Sans",
      },
    });
    expect(vars["--radius-scale"]).toBe("0");
    expect(vars["--border-width"]).toBe("2px");
    expect(vars["--shadow"]).toBe("4px 4px 0 var(--border)");
    expect(vars["--font-display"]).toContain("Pixelify Sans");
  });

  it("only blurs when the theme asks for it", () => {
    const vars = themeToCssVars({ ...base, options: { "surface-style": "blur" } });
    expect(vars["--surface-blur"]).toBe("blur(12px)");
  });
});
