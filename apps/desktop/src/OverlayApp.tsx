import { useCallback, useEffect, useRef, useState } from "react";
import {
  FOX_SIZES,
  FoxView,
  STATE_DURATION_MS,
  isFoxState,
  type FoxState,
} from "@frosty/character";
import { SpeechChip } from "@frosty/ui";

import { api, type DecisionPayload } from "./bridge";
import { useConfig, useTauriEvent, useTheme } from "./hooks";

interface FoxPosition {
  right: number;
  bottom: number;
}

const DEFAULT_POSITION: FoxPosition = { right: 24, bottom: 24 };
const EDGE_SNAP_PX = 48;
const EDGE_MARGIN_PX = 24;
const CHIP_MS = 4000;

/** Compact mode: the fox, its dock, and a speech chip on a transparent overlay. */
export function OverlayApp() {
  const config = useConfig();
  useTheme(config);

  const [shown, setShown] = useState(false);
  const [state, setState] = useState<FoxState>("idle");
  const [intensity, setIntensity] = useState(0.5);
  const [playKey, setPlayKey] = useState(0);
  const [chip, setChip] = useState<{ id: number; text: string } | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const [position, setPosition] = useState<FoxPosition>(DEFAULT_POSITION);
  const stateTimer = useRef<number | undefined>(undefined);
  const chipTimer = useRef<number | undefined>(undefined);
  const dockRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; start: FoxPosition } | null>(null);

  const foxSize = FOX_SIZES[config?.character.size ?? "medium"];

  const play = useCallback((next: FoxState, amount: number, then?: () => void) => {
    window.clearTimeout(stateTimer.current);
    setState(next);
    setIntensity(amount);
    setPlayKey((k) => k + 1);
    const duration = STATE_DURATION_MS[next];
    if (duration !== null) {
      stateTimer.current = window.setTimeout(() => {
        then?.();
        if (next !== "dismiss_out") setState("idle");
      }, duration);
    }
  }, []);

  useEffect(() => {
    void api.getFoxPosition().then((saved) => {
      if (!saved) return;
      try {
        const parsed = JSON.parse(saved) as Partial<FoxPosition>;
        if (typeof parsed.right === "number" && typeof parsed.bottom === "number") {
          setPosition({ right: parsed.right, bottom: parsed.bottom });
        }
      } catch {
        // A bad saved position falls back to the default.
      }
    });
  }, []);

  useTauriEvent<void>("frosty://summon", () => {
    setShown(true);
    play("summon_in", 0.5);
  });

  const dismiss = useCallback(() => {
    play("dismiss_out", 0.5, () => {
      setShown(false);
      void api.overlayHidden();
    });
  }, [play]);

  useTauriEvent<void>("frosty://dismiss-request", dismiss);

  useTauriEvent<void>("frosty://focus-dock", () => {
    dockRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
  });

  useTauriEvent<DecisionPayload>("frosty://decision", ({ decision }) => {
    if (decision.kind !== "play" || !shown) return;
    const { animation, intensity: amount, text, show_chip } = decision;
    if (animation && isFoxState(animation)) {
      play(animation, amount);
    }
    if (show_chip && text) {
      window.clearTimeout(chipTimer.current);
      setChip({ id: Date.now(), text });
      setAnnouncement(text);
      chipTimer.current = window.setTimeout(() => setChip(null), CHIP_MS);
    }
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && shown) {
        e.preventDefault();
        void api.toggleSummon();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shown]);

  // After a summon, report once the first frame is on screen (two animation
  // frames after the overlay is shown) so the core can log the latency.
  useEffect(() => {
    if (!shown) return;
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => void api.summonPainted());
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, [shown]);

  // Tell the core which rectangles take the mouse; everything else is
  // click-through. Runs through each animation so the region follows it.
  const layoutKey = [
    shown,
    state,
    playKey,
    chip?.id,
    position.right,
    position.bottom,
    foxSize,
  ].join("|");
  useEffect(() => {
    let frame = 0;
    let last = "";
    const until = performance.now() + 900;
    const tick = () => {
      const rects = shown
        ? [...document.querySelectorAll<HTMLElement>("[data-hit]")].map((el) => {
            const r = el.getBoundingClientRect();
            return { x: r.x - 8, y: r.y - 8, w: r.width + 16, h: r.height + 16 };
          })
        : [];
      const signature = rects.map((r) => [r.x, r.y, r.w, r.h].map(Math.round).join(",")).join(";");
      if (signature !== last) {
        last = signature;
        void api.reportHitRects(rects);
      }
      if (performance.now() < until) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [layoutKey, shown]);

  const onPointerDown = (e: React.PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, y: e.clientY, start: position };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    setPosition({
      right: Math.max(0, Math.min(window.innerWidth - foxSize, d.start.right - (e.clientX - d.x))),
      bottom: Math.max(
        0,
        Math.min(window.innerHeight - foxSize, d.start.bottom - (e.clientY - d.y)),
      ),
    });
  };
  const onPointerUp = () => {
    if (!drag.current) return;
    drag.current = null;
    const snapped = {
      right: position.right < EDGE_SNAP_PX ? EDGE_MARGIN_PX : position.right,
      bottom: position.bottom < EDGE_SNAP_PX ? EDGE_MARGIN_PX : position.bottom,
    };
    setPosition(snapped);
    void api.saveFoxPosition(JSON.stringify(snapped));
  };

  return (
    <div className="overlay-root">
      <div className="sr-only" role="status" aria-live="polite">
        {announcement}
      </div>
      {shown ? (
        <div className="overlay-stack" style={{ right: position.right, bottom: position.bottom }}>
          {chip ? (
            <div data-hit className="overlay-chip" key={chip.id}>
              <SpeechChip>{chip.text}</SpeechChip>
            </div>
          ) : null}
          <div
            data-hit
            className="overlay-fox"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
          >
            <FoxView state={state} intensity={intensity} size={foxSize} playKey={playKey} />
          </div>
          <div data-hit className="dock" ref={dockRef} role="toolbar" aria-label="Frosty dock">
            <button
              type="button"
              className="dock-button"
              aria-label="Open the module manager"
              title="Modules"
              onClick={() => void api.openMain("modules")}
            >
              +
            </button>
            <button
              type="button"
              className="dock-button"
              aria-label="Open fullscreen"
              title="Fullscreen"
              onClick={() => void api.toggleFullscreen()}
            >
              ⤢
            </button>
            <button
              type="button"
              className="dock-button"
              aria-label="Dismiss Frosty"
              title="Dismiss"
              onClick={() => void api.toggleSummon()}
            >
              ×
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
