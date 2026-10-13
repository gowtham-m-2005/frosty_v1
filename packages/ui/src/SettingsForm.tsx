import { useState } from "react";

import { Banner, Button, Field, Segmented, Toggle } from "./components";

export type JsonSchema = {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
  enum?: unknown[];
  minimum?: number;
  maximum?: number;
  format?: string;
  items?: JsonSchema;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  "x-frosty-widget"?: string;
  "x-frosty-group"?: string;
  "x-frosty-order"?: number;
  [key: string]: unknown;
};

export type Widget =
  | "toggle"
  | "text"
  | "number"
  | "select"
  | "multi-select"
  | "time"
  | "date"
  | "color"
  | "path"
  | "list"
  | "keybinding";

const KNOWN_WIDGETS: Widget[] = [
  "toggle",
  "text",
  "number",
  "select",
  "multi-select",
  "time",
  "date",
  "color",
  "path",
  "list",
  "keybinding",
];

/** Picks the control for a property: an explicit hint, else from the schema. */
export function widgetFor(prop: JsonSchema): Widget {
  const hinted = prop["x-frosty-widget"];
  if (hinted && (KNOWN_WIDGETS as string[]).includes(hinted)) return hinted as Widget;
  if (prop.enum && prop.type !== "array") return "select";
  if (prop.type === "array") return prop.items?.enum ? "multi-select" : "list";
  if (prop.type === "boolean") return "toggle";
  if (prop.type === "integer" || prop.type === "number") return "number";
  if (prop.format === "time") return "time";
  if (prop.format === "date") return "date";
  if (prop.format === "color") return "color";
  return "text";
}

interface Entry {
  key: string;
  prop: JsonSchema;
  group: string;
}

/** Properties in display order, grouped by `x-frosty-group`. */
export function orderedEntries(schema: JsonSchema): Entry[] {
  const props = Object.entries(schema.properties ?? {});
  return props
    .map(([key, prop], index) => ({ key, prop, index, order: prop["x-frosty-order"] ?? index }))
    .sort((a, b) => a.order - b.order || a.index - b.index)
    .map(({ key, prop }) => ({ key, prop, group: prop["x-frosty-group"] ?? "" }));
}

function ListEditor({
  value,
  onChange,
  label,
}: {
  value: string[];
  onChange: (next: string[]) => void;
  label: string;
}) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const text = draft.trim();
    if (text) onChange([...value, text]);
    setDraft("");
  };
  return (
    <div className="f-list-editor">
      <ul>
        {value.map((item, i) => (
          <li key={`${item}-${i}`}>
            <span>{item}</span>
            <Button
              variant="ghost"
              aria-label={`Remove ${item}`}
              onClick={() => onChange(value.filter((_, j) => j !== i))}
            >
              Remove
            </Button>
          </li>
        ))}
      </ul>
      <div className="f-list-add">
        <input
          className="f-input"
          aria-label={`Add to ${label}`}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button onClick={add}>Add</Button>
      </div>
    </div>
  );
}

/** Records a key combination the way Windows names it, for example Ctrl+Alt+F. */
function KeybindingRecorder({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (next: string) => void;
  label: string;
}) {
  return (
    <input
      className="f-input"
      aria-label={label}
      value={value}
      placeholder="Press a key combination"
      readOnly
      onKeyDown={(e) => {
        e.preventDefault();
        if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
        const parts = [];
        if (e.ctrlKey) parts.push("Ctrl");
        if (e.altKey) parts.push("Alt");
        if (e.shiftKey) parts.push("Shift");
        if (e.metaKey) parts.push("Super");
        const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
        if (parts.length === 0) return; // a bare key would hijack typing
        onChange([...parts, key].join("+"));
      }}
    />
  );
}

function Control({
  name,
  prop,
  value,
  onChange,
}: {
  name: string;
  prop: JsonSchema;
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const label = prop.title ?? name;
  switch (widgetFor(prop)) {
    case "toggle":
      return <Toggle label={label} checked={value === true} onChange={onChange} />;
    case "number":
      return (
        <input
          className="f-input"
          type="number"
          aria-label={label}
          min={prop.minimum}
          max={prop.maximum}
          step={prop.type === "integer" ? 1 : "any"}
          value={typeof value === "number" ? value : ""}
          onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
        />
      );
    case "select": {
      const options = (prop.enum ?? []).map((v) => ({ value: String(v), label: String(v) }));
      return options.length <= 4 ? (
        <Segmented
          label={label}
          value={String(value ?? options[0]?.value ?? "")}
          options={options}
          onChange={onChange}
        />
      ) : (
        <select
          className="f-select"
          aria-label={label}
          value={String(value ?? "")}
          onChange={(e) => onChange(e.target.value)}
        >
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      );
    }
    case "multi-select": {
      const chosen = Array.isArray(value) ? (value as string[]) : [];
      return (
        <div role="group" aria-label={label} className="f-checks">
          {(prop.items?.enum ?? []).map((option) => {
            const text = String(option);
            return (
              <label key={text}>
                <input
                  type="checkbox"
                  checked={chosen.includes(text)}
                  onChange={(e) =>
                    onChange(
                      e.target.checked ? [...chosen, text] : chosen.filter((c) => c !== text),
                    )
                  }
                />{" "}
                {text}
              </label>
            );
          })}
        </div>
      );
    }
    case "time":
    case "date":
      return (
        <input
          className="f-input"
          type={widgetFor(prop)}
          aria-label={label}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "color":
      return (
        <input
          type="color"
          aria-label={label}
          value={typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : "#000000"}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "list":
      return (
        <ListEditor
          label={label}
          value={Array.isArray(value) ? (value as string[]) : []}
          onChange={onChange}
        />
      );
    case "keybinding":
      return (
        <KeybindingRecorder
          label={label}
          value={typeof value === "string" ? value : ""}
          onChange={onChange}
        />
      );
    case "path":
    case "text":
    default:
      return (
        <input
          className="f-input"
          aria-label={label}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}

export interface SettingsFormProps {
  schema: JsonSchema;
  values: Record<string, unknown>;
  /** Called with only the keys that changed. Throw to show an error. */
  onSave: (changes: Record<string, unknown>) => Promise<void> | void;
}

/**
 * The settings form generated from a capability's JSON Schema
 * (docs/PROJECT_SPEC.md 8.7). Saving writes the capability's config file.
 */
export function SettingsForm({ schema, values, onSave }: SettingsFormProps) {
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const entries = orderedEntries(schema);
  const current = (key: string) =>
    key in draft ? draft[key] : (values[key] ?? schema.properties?.[key]?.default);
  const dirty = Object.keys(draft).length > 0;

  const save = async () => {
    setError(null);
    try {
      await onSave(draft);
      setDraft({});
      setSaved(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <form
      className="f-settings-form"
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      {entries.length === 0 ? <p className="f-field-hint">This module has no settings.</p> : null}
      {entries.map(({ key, prop, group }, index) => {
        const previous = index > 0 ? entries[index - 1]?.group : "";
        const heading = group && group !== previous ? <h3 key={`g-${group}`}>{group}</h3> : null;
        return (
          <div key={key}>
            {heading}
            <Field label={prop.title ?? key} hint={prop.description}>
              <Control
                name={key}
                prop={prop}
                value={current(key)}
                onChange={(next) => {
                  setSaved(false);
                  setDraft((d) => {
                    const copy = { ...d };
                    if (next === undefined) delete copy[key];
                    else copy[key] = next;
                    return copy;
                  });
                }}
              />
            </Field>
          </div>
        );
      })}
      {error ? <Banner>{error}</Banner> : null}
      {saved && !dirty ? (
        <p role="status" className="f-field-hint">
          Saved.
        </p>
      ) : null}
      <div className="f-form-actions">
        <Button type="submit" variant="primary" disabled={!dirty}>
          Save
        </Button>
        <Button disabled={!dirty} onClick={() => setDraft({})}>
          Discard changes
        </Button>
      </div>
    </form>
  );
}
