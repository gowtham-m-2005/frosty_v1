import type { ButtonHTMLAttributes, ReactNode } from "react";
import { useId } from "react";

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

export function Button({
  variant = "secondary",
  className = "",
  type = "button",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return <button type={type} className={`f-button f-button-${variant} ${className}`} {...rest} />;
}

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="f-toggle"
      onClick={() => onChange(!checked)}
    >
      <span className="f-toggle-knob" />
    </button>
  );
}

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

/** A small radio group drawn as joined buttons. Arrow keys move the choice. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="f-segmented">
      {options.map((option, index) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          tabIndex={option.value === value ? 0 : -1}
          className="f-segment"
          onClick={() => onChange(option.value)}
          onKeyDown={(e) => {
            const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
            if (step === 0) return;
            e.preventDefault();
            const next = options[(index + step + options.length) % options.length];
            if (next) onChange(next.value);
          }}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Select<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (next: T) => void;
  label: string;
}) {
  return (
    <select
      className="f-select"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

export function Banner({
  level = "error",
  children,
}: {
  level?: "error" | "info";
  children: ReactNode;
}) {
  // The label says what kind of message it is, so color is not the only cue.
  return (
    <div role={level === "error" ? "alert" : "status"} className={`f-banner f-banner-${level}`}>
      <strong>{level === "error" ? "Problem: " : "Note: "}</strong>
      {children}
    </div>
  );
}

export function SpeechChip({ children }: { children: ReactNode }) {
  return <div className="f-chip">{children}</div>;
}

/** A settings row: label and description on the left, control on the right. */
export function Field({
  label,
  hint,
  configKey,
  children,
}: {
  label: string;
  hint?: string;
  /** The `frosty.toml` key behind this control, shown so UI and file stay linked. */
  configKey?: string;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <div className="f-field" title={configKey ? `In config: ${configKey}` : undefined}>
      <div className="f-field-text">
        <span id={id} className="f-field-label">
          {label}
        </span>
        {hint ? <span className="f-field-hint">{hint}</span> : null}
        {configKey ? <code className="f-field-key">{configKey}</code> : null}
      </div>
      <div className="f-field-control" aria-labelledby={id}>
        {children}
      </div>
    </div>
  );
}
