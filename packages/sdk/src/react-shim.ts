/**
 * Stands in for `react` when a capability is bundled, so the capability uses
 * the shell's React instead of shipping its own (two copies break hooks).
 * The build tool aliases `react` to this file.
 */
import type * as ReactTypes from "react";

const React = (globalThis as { __frosty?: { React: typeof ReactTypes } }).__frosty
  ?.React as typeof ReactTypes;

if (!React) {
  throw new Error("Frosty shell globals are missing; this code must run inside Frosty");
}

export default React;
export const Children = React.Children;
export const Fragment = React.Fragment;
export const StrictMode = React.StrictMode;
export const Suspense = React.Suspense;
export const createContext = React.createContext;
export const createElement = React.createElement;
export const forwardRef = React.forwardRef;
export const isValidElement = React.isValidElement;
export const lazy = React.lazy;
export const memo = React.memo;
export const startTransition = React.startTransition;
export const useCallback = React.useCallback;
export const useContext = React.useContext;
export const useDeferredValue = React.useDeferredValue;
export const useEffect = React.useEffect;
export const useId = React.useId;
export const useLayoutEffect = React.useLayoutEffect;
export const useMemo = React.useMemo;
export const useReducer = React.useReducer;
export const useRef = React.useRef;
export const useState = React.useState;
export const useTransition = React.useTransition;
