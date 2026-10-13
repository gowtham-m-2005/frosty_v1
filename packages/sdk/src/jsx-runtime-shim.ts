/**
 * `react/jsx-runtime` for capability bundles, built on the shell's React.
 * The build tool aliases `react/jsx-runtime` to this file.
 */
import type * as ReactTypes from "react";

const React = (globalThis as { __frosty?: { React: typeof ReactTypes } }).__frosty
  ?.React as typeof ReactTypes;

if (!React) {
  throw new Error("Frosty shell globals are missing; this code must run inside Frosty");
}

export const Fragment = React.Fragment;

export function jsx(
  type: ReactTypes.ElementType,
  props: Record<string, unknown> & { children?: unknown },
  key?: ReactTypes.Key,
): ReactTypes.ReactElement {
  const { children, ...rest } = props;
  const withKey = key === undefined ? rest : { ...rest, key };
  if (children === undefined) return React.createElement(type, withKey);
  if (Array.isArray(children))
    return React.createElement(type, withKey, ...(children as ReactTypes.ReactNode[]));
  return React.createElement(type, withKey, children as ReactTypes.ReactNode);
}

export const jsxs = jsx;
export const jsxDEV = jsx;
