/** What `resolve_theme` returns from the Rust core. */
export interface ResolvedTheme {
  id: string;
  name: string;
  mode: "light" | "dark";
  /** True when the theme does not support the mode that was asked for. */
  fallback: boolean;
  tokens: Record<string, string>;
  options: Record<string, string>;
}

const SOFT_SHADOW = "0 6px 20px rgba(0, 0, 0, 0.18)";

/**
 * Turns a resolved theme into CSS custom properties. Components read only
 * these, never raw colors (docs/DESIGN_SPEC.md 7.2).
 */
export function themeToCssVars(theme: ResolvedTheme): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [token, value] of Object.entries(theme.tokens)) {
    vars[`--${token}`] = value;
  }

  const o = theme.options;
  vars["--radius-scale"] = o["radius-scale"] ?? "1";
  vars["--border-width"] = `${o["border-width"] ?? "1"}px`;
  vars["--shadow"] = o["shadow-style"] === "hard" ? "4px 4px 0 var(--border)" : SOFT_SHADOW;
  vars["--surface-blur"] = o["surface-style"] === "blur" ? "blur(12px)" : "none";
  vars["--font-display"] = o["font-display"]
    ? `"${o["font-display"]}", var(--font-body)`
    : "var(--font-body)";
  if (o["fox-tint"]) {
    vars["--fox-tint"] = o["fox-tint"];
  }
  return vars;
}

export function applyTheme(root: HTMLElement, theme: ResolvedTheme): void {
  for (const [name, value] of Object.entries(themeToCssVars(theme))) {
    root.style.setProperty(name, value);
  }
  root.dataset.theme = theme.id;
  root.dataset.mode = theme.mode;
}
