import type { CSSProperties } from "react";

import "./fox.css";

/** Reaction states from docs/DESIGN_SPEC.md 2.4. */
export type ReactionName =
  | "wave"
  | "happy_small"
  | "celebrate"
  | "curious_tilt"
  | "nudge"
  | "sleepy"
  | "oops"
  | "thinking"
  | "bye";

export type TransitionName = "summon_in" | "dismiss_out" | "mode_expand" | "mode_collapse";

export type FoxState = "idle" | ReactionName | TransitionName;

export const FOX_SIZES = { small: 112, medium: 160, large: 224 } as const;
export type FoxSizeName = keyof typeof FOX_SIZES;

/** How long each state plays before the fox returns to idle. `null` holds. */
export const STATE_DURATION_MS: Record<FoxState, number | null> = {
  idle: null,
  summon_in: 480,
  dismiss_out: 280,
  mode_expand: 320,
  mode_collapse: 320,
  wave: 1000,
  happy_small: 800,
  celebrate: 1100,
  curious_tilt: 1200,
  nudge: 1200,
  sleepy: null,
  oops: 600,
  thinking: null,
  bye: 1000,
};

export function isFoxState(value: string): value is FoxState {
  return value in STATE_DURATION_MS;
}

export interface FoxViewProps {
  state?: FoxState;
  /** 0 to 1. Scales amplitude, not the base pose. */
  intensity?: number;
  /** Logical pixels. */
  size?: number;
  /** When false the idle loops stop (nothing loops while hidden). */
  visible?: boolean;
  /** Change to replay the same state. */
  playKey?: number;
}

/**
 * Placeholder fox: flat shapes behind the skin contract, so real art can
 * replace it without touching callers. Decorative, hidden from screen readers.
 */
export function FoxView({
  state = "idle",
  intensity = 0.5,
  size = 160,
  visible = true,
  playKey = 0,
}: FoxViewProps) {
  const style = { width: size, height: size, "--intensity": intensity } as CSSProperties;
  return (
    <div className="fox" data-state={state} data-visible={visible ? "true" : "false"} style={style}>
      <div className="fox-motion" key={`${state}-${playKey}`}>
        <svg viewBox="0 0 64 64" aria-hidden="true" focusable="false">
          <g className="fox-tail">
            <path d="M42 52 Q63 56 60 32 Q53 43 42 44 Z" className="fox-fur" />
            <path d="M60 32 Q58 38 54 41 Q58 40 61 36 Z" className="fox-tip" />
          </g>
          <g className="fox-body">
            <path d="M19 60 L19 42 Q19 33 32 33 Q45 33 45 42 L45 60 Z" className="fox-fur" />
            <rect x="19" y="39" width="26" height="5" className="fox-scarf" />
          </g>
          <g className="fox-head">
            <path
              d="M15 27 L17 7 L28 16 L36 16 L47 7 L49 27 Q49 41 32 41 Q15 41 15 27 Z"
              className="fox-fur"
            />
            <path d="M19 12 L25 17 L20 21 Z" className="fox-ear-inner" />
            <path d="M45 12 L39 17 L44 21 Z" className="fox-ear-inner fox-ear-twitch" />
            <rect x="23" y="26" width="4" height="5" className="fox-eye" />
            <rect x="37" y="26" width="4" height="5" className="fox-eye" />
            <rect x="30" y="33" width="4" height="3" className="fox-nose" />
          </g>
        </svg>
      </div>
    </div>
  );
}
