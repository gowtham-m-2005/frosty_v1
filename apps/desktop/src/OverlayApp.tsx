import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  FOX_SIZES,
  FoxView,
  STATE_DURATION_MS,
  isFoxState,
  type FoxState,
} from "@frosty/character";
import { Button, ModuleCardFrame, SpeechChip } from "@frosty/ui";

import {
  api,
  type CapabilityInfo,
  type DecisionPayload,
  type NudgeInfo,
  type ToolInfo,
} from "./bridge";
import { CapabilitySlot, useCapabilities } from "./capabilities";
import { useConfig, useTauriEvent, useTheme } from "./hooks";

interface FoxPosition {
  right: number;
  bottom: number;
}

const DEFAULT_POSITION: FoxPosition = { right: 24, bottom: 24 };
const EDGE_SNAP_PX = 48;
const EDGE_MARGIN_PX = 24;
const CHIP_MS = 4000;
const CARD_W = 380;
const CARD_H = 520;
const DOCK_H = 56;
const GAP = 12;
const DOCK_VISIBLE = 6;

interface QuickTarget {
  capability: CapabilityInfo;
  tool: ToolInfo;
}

/** One small control above the dock: a text field, stepper, toggle, or button. */
function QuickActionTray({
  target,
  onDone,
  onCancel,
}: {
  target: QuickTarget;
  onDone: () => void;
  onCancel: () => void;
}) {
  const { tool } = target;
  const control = tool.quick_action_control ?? "text";
  const [text, setText] = useState("");
  const [amount, setAmount] = useState<number>(tool.primary_min ?? 1);
  const [on, setOn] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const confirm = () => {
    const field = tool.primary_field;
    const input: Record<string, unknown> = {};
    if (control === "text") {
      if (!text.trim()) return;
      if (field) input[field] = text.trim();
    } else if (control === "stepper") {
      if (field) input[field] = amount;
    } else if (control === "toggle") {
      if (field) input[field] = on;
    }
    setBusy(true);
    setError(null);
    api
      .callTool(tool.id, input)
      .then(() => {
        onDone();
        // Hand focus back to what the user was doing.
        void api.restorePreviousFocus();
      })
      .catch((e: unknown) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div
      data-hit
      className="quick-tray"
      role="group"
      aria-label={tool.title}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onCancel();
        }
      }}
    >
      {control === "text" ? (
        <input
          className="f-input"
          autoFocus
          aria-label={tool.title}
          placeholder={tool.title}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && confirm()}
        />
      ) : null}
      {control === "stepper" ? (
        <input
          className="f-input"
          type="number"
          autoFocus
          aria-label={tool.title}
          min={tool.primary_min ?? undefined}
          max={tool.primary_max ?? undefined}
          value={amount}
          onChange={(e) => setAmount(Number(e.target.value))}
          onKeyDown={(e) => e.key === "Enter" && confirm()}
        />
      ) : null}
      {control === "toggle" ? (
        <label>
          <input type="checkbox" checked={on} onChange={(e) => setOn(e.target.checked)} />{" "}
          {tool.title}
        </label>
      ) : null}
      <Button variant="primary" disabled={busy} onClick={confirm}>
        {control === "button" ? tool.title : "Add"}
      </Button>
      {error ? (
        <span role="alert" className="quick-error">
          {error}
        </span>
      ) : null}
    </div>
  );
}

/** Compact mode: the fox, its dock, a speech chip, and the open card. */
export function OverlayApp() {
  const config = useConfig();
  useTheme(config);
  const { capabilities } = useCapabilities();

  const [shown, setShown] = useState(false);
  const [state, setState] = useState<FoxState>("idle");
  const [intensity, setIntensity] = useState(0.5);
  const [playKey, setPlayKey] = useState(0);
  const [chip, setChip] = useState<{ id: number; text: string } | null>(null);
  const [nudge, setNudge] = useState<NudgeInfo | null>(null);
  // Appeared only to deliver a reminder: fox and chip, no dock.
  const [minimal, setMinimal] = useState(false);
  const [announcement, setAnnouncement] = useState("");
  const [position, setPosition] = useState<FoxPosition>(DEFAULT_POSITION);
  const [openCard, setOpenCard] = useState<string | null>(null);
  const [quick, setQuick] = useState<QuickTarget | null>(null);
  const stateTimer = useRef<number | undefined>(undefined);
  const chipTimer = useRef<number | undefined>(undefined);
  const nudgeTimer = useRef<number | undefined>(undefined);
  const minimalRef = useRef(false);
  const dockRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; start: FoxPosition } | null>(null);

  const foxSize = FOX_SIZES[config?.character.size ?? "medium"];

  // Enabled modules with views, in the user's dock order.
  const dockItems = useMemo(() => {
    const order = config?.capabilities.dock_order ?? [];
    const rank = (id: string) => {
      const i = order.indexOf(id);
      return i === -1 ? order.length : i;
    };
    return capabilities
      .filter((c) => c.state === "enabled" && c.views_url)
      .sort((a, b) => rank(a.id) - rank(b.id));
  }, [capabilities, config?.capabilities.dock_order]);

  // A module that was turned off or crashed no longer has an open card.
  const openInfo = dockItems.find((c) => c.id === openCard) ?? null;
  const activeCard = openInfo ? openCard : null;

  const play = useCallback((next: FoxState, amount: number, then?: () => void) => {
    window.clearTimeout(stateTimer.current);
    setState(next);
    setIntensity(amount);
    setPlayKey((k) => k + 1);
    const duration = STATE_DURATION_MS[next];
    if (duration !== null) {
      stateTimer.current = window.setTimeout(() => {
        if (next !== "dismiss_out") setState("idle");
        then?.();
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

  const dismiss = useCallback(() => {
    setOpenCard(null);
    setQuick(null);
    setNudge(null);
    window.clearTimeout(nudgeTimer.current);
    play("dismiss_out", 0.5, () => {
      setShown(false);
      setMinimal(false);
      minimalRef.current = false;
      void api.overlayHidden();
    });
  }, [play]);

  // Moving a reminder out of sight: it stays in the notification history.
  const closeNudge = useCallback(() => {
    window.clearTimeout(nudgeTimer.current);
    setNudge(null);
    if (minimalRef.current) dismiss();
  }, [dismiss]);

  useTauriEvent<void>("frosty://summon", () => {
    setMinimal(false);
    minimalRef.current = false;
    setShown(true);
    play("summon_in", 0.5);
  });

  // The first press closes an open card or tray, the second dismisses.
  useTauriEvent<void>("frosty://dismiss-request", () => {
    if (quick) setQuick(null);
    else if (activeCard) setOpenCard(null);
    else dismiss();
  });

  // Summoning while visible brings up the dock, even after a reminder.
  useTauriEvent<void>("frosty://focus-dock", () => {
    setMinimal(false);
    minimalRef.current = false;
    window.setTimeout(() => {
      dockRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    }, 0);
  });

  // A reminder was delivered: Frosty appears (if hidden) and nudges.
  useTauriEvent<NudgeInfo>("frosty://notification", (n) => {
    setNudge(n);
    setAnnouncement(n.title);
    window.clearTimeout(nudgeTimer.current);
    nudgeTimer.current = window.setTimeout(() => closeNudge(), n.seconds * 1000);
    if (!shown) {
      minimalRef.current = true;
      setMinimal(true);
      setShown(true);
      play("summon_in", 0.5, () => play("nudge", 0.6));
    } else {
      play("nudge", 0.6);
    }
  });

  useTauriEvent<DecisionPayload>("frosty://decision", ({ event, decision }) => {
    if (decision.kind !== "play" || !shown) return;
    const { animation, intensity: amount, text, show_chip } = decision;
    // A reminder shows its own chip, with its actions.
    if (event === "core.notification") return;
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
      if (e.key !== "Escape" || !shown) return;
      e.preventDefault();
      if (quick) setQuick(null);
      else if (activeCard) setOpenCard(null);
      else if (nudge) closeNudge();
      else void api.toggleSummon();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shown, activeCard, quick, nudge, closeNudge]);

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
    minimal,
    state,
    playKey,
    chip?.id,
    nudge?.id,
    quick?.tool.id,
    activeCard,
    dockItems.length,
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

  // The card sits beside the fox on whichever side has room, else above it
  // (docs/DESIGN_SPEC.md 5.2).
  const cardStyle = (() => {
    const roomLeft = window.innerWidth - position.right - foxSize;
    const height = Math.min(CARD_H, window.innerHeight - EDGE_MARGIN_PX * 2);
    if (roomLeft >= CARD_W + GAP) {
      return {
        right: position.right + foxSize + GAP,
        bottom: position.bottom,
        width: CARD_W,
        height,
      };
    }
    const bottom = position.bottom + foxSize + DOCK_H + GAP;
    return {
      right: position.right,
      bottom,
      width: CARD_W,
      height: Math.min(CARD_H, window.innerHeight - bottom - EDGE_MARGIN_PX),
    };
  })();

  const visibleItems = dockItems.slice(0, DOCK_VISIBLE);
  const overflow = dockItems.slice(DOCK_VISIBLE);

  return (
    <div className="overlay-root">
      <div className="sr-only" role="status" aria-live="polite">
        {announcement}
      </div>
      {shown ? (
        <>
          {openInfo && !minimal ? (
            <div data-hit className="overlay-card" style={cardStyle}>
              <ModuleCardFrame
                title={openInfo.name}
                onClose={() => setOpenCard(null)}
                onExpand={() => {
                  setOpenCard(null);
                  void api.openMain(`module:${openInfo.id}`);
                }}
              >
                <CapabilitySlot info={openInfo} slot="card" />
              </ModuleCardFrame>
            </div>
          ) : null}
          <div className="overlay-stack" style={{ right: position.right, bottom: position.bottom }}>
            {nudge ? (
              <div data-hit className="overlay-nudge" key={nudge.id}>
                <SpeechChip>
                  <strong>{nudge.title}</strong>
                  {nudge.body ? <div className="nudge-body">{nudge.body}</div> : null}
                  <div className="nudge-actions">
                    {nudge.actions.slice(0, 2).map((action, index) => (
                      <Button
                        key={`${action.label}-${index}`}
                        variant={index === 0 ? "primary" : "secondary"}
                        onClick={() => {
                          void api.runNotificationAction(nudge.id, index).catch(() => undefined);
                          closeNudge();
                        }}
                      >
                        {action.label}
                      </Button>
                    ))}
                    <Button
                      variant="ghost"
                      aria-label="Dismiss reminder"
                      onClick={() => {
                        void api.dismissNotification(nudge.id);
                        closeNudge();
                      }}
                    >
                      ×
                    </Button>
                  </div>
                </SpeechChip>
              </div>
            ) : chip ? (
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
            {quick && !minimal ? (
              <QuickActionTray
                target={quick}
                onDone={() => setQuick(null)}
                onCancel={() => setQuick(null)}
              />
            ) : null}
            {minimal ? null : (
              <div data-hit className="dock" ref={dockRef} role="toolbar" aria-label="Frosty dock">
                {visibleItems.map((item) => {
                  const quickTool = item.tools.find((t) => t.quick_action);
                  return (
                    <span key={item.id} className="dock-slot">
                      <button
                        type="button"
                        className="dock-button"
                        aria-label={item.badge > 0 ? `${item.name}, ${item.badge}` : item.name}
                        aria-pressed={activeCard === item.id}
                        title={item.name}
                        onClick={() => {
                          setQuick(null);
                          setOpenCard((cur) => (cur === item.id ? null : item.id));
                        }}
                      >
                        {item.icon_url ? (
                          <span
                            className="dock-icon"
                            style={{
                              maskImage: `url(${item.icon_url})`,
                              WebkitMaskImage: `url(${item.icon_url})`,
                            }}
                          />
                        ) : (
                          item.name.slice(0, 1)
                        )}
                        {item.badge > 0 ? (
                          <span className="dock-badge" aria-hidden="true">
                            {item.badge > 9 ? "9+" : item.badge}
                          </span>
                        ) : null}
                      </button>
                      {quickTool ? (
                        <button
                          type="button"
                          className="dock-quick"
                          aria-label={quickTool.title}
                          title={quickTool.title}
                          onClick={() => {
                            setOpenCard(null);
                            setQuick({ capability: item, tool: quickTool });
                          }}
                        >
                          +
                        </button>
                      ) : null}
                    </span>
                  );
                })}
                {overflow.length > 0 ? (
                  <button
                    type="button"
                    className="dock-button"
                    aria-label={`${overflow.length} more modules`}
                    title="More modules"
                    onClick={() => void api.openMain("modules")}
                  >
                    …
                  </button>
                ) : null}
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
            )}
          </div>
        </>
      ) : null}
    </div>
  );
}
