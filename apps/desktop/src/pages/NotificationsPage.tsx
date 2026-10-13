import { useCallback, useEffect, useState } from "react";
import { Banner, Button, Toggle } from "@frosty/ui";

import { api, type DndStatus, type NotificationRecord } from "../bridge";
import { useTauriEvent } from "../hooks";

const STATE_TEXT: Record<NotificationRecord["state"], string> = {
  held: "Waiting",
  delivered: "Shown",
  dismissed: "Dismissed",
  dropped_muted: "Muted, not shown",
  dropped_rate_limited: "Too many at once, not shown",
};

const HOLD_TEXT: Record<string, string> = {
  do_not_disturb: "Held for do not disturb",
  fullscreen_app: "Held while a fullscreen app is open",
};

function when(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === today.toDateString() ? time : `${d.toLocaleDateString()} ${time}`;
}

/** Everything Frosty was asked to show, including what was held or dropped. */
export function NotificationsPage() {
  const [items, setItems] = useState<NotificationRecord[]>([]);
  const [dnd, setDnd] = useState<DndStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const refresh = useCallback(() => {
    void api.listNotifications(100).then(setItems);
    void api.getDnd().then(setDnd);
  }, []);

  useEffect(() => {
    refresh();
    // Opening the page counts as reading what is there.
    void api.markNotificationsRead();
  }, [refresh]);
  useTauriEvent("frosty://notifications-changed", refresh);
  useTauriEvent("frosty://dnd", refresh);

  return (
    <div className="notifications">
      <div className="f-field">
        <div className="f-field-text">
          <span className="f-field-label">Do not disturb</span>
          <span className="f-field-hint">
            {dnd?.manual
              ? "On until you turn it off."
              : dnd?.active
                ? "On, because of your nightly schedule."
                : "Off. Reminders appear as they are due."}
            {dnd?.fullscreen_app ? " A fullscreen app is open, so reminders are waiting." : ""}
          </span>
        </div>
        <Toggle
          label="Do not disturb"
          checked={dnd?.manual ?? false}
          onChange={(on) => void api.setDndManual(on)}
        />
      </div>

      {problem ? <Banner>{problem}</Banner> : null}

      {items.length === 0 ? (
        <section className="empty-state">
          <h2>No notifications yet</h2>
          <p>Reminders from your modules show up here, including ones that were held back.</p>
        </section>
      ) : (
        <ul className="notification-list">
          {items.map((n) => (
            <li key={n.id} className={`notification notification-${n.state}`}>
              <div>
                <strong>{n.title}</strong>
                {n.body ? <div className="muted">{n.body}</div> : null}
                <div className="muted">
                  {n.owner} · {when(n.created_at)} ·{" "}
                  {n.state === "held" && n.hold_reason
                    ? (HOLD_TEXT[n.hold_reason] ?? STATE_TEXT.held)
                    : STATE_TEXT[n.state]}
                </div>
              </div>
              <div className="notification-actions">
                {n.state !== "dismissed" && !n.state.startsWith("dropped")
                  ? n.actions.map((action, index) => (
                      <Button
                        key={`${action.label}-${index}`}
                        onClick={() =>
                          api
                            .runNotificationAction(n.id, index)
                            .then(refresh)
                            .catch((e: unknown) => setProblem(String(e)))
                        }
                      >
                        {action.label}
                      </Button>
                    ))
                  : null}
                {n.state === "held" || n.state === "delivered" ? (
                  <Button
                    variant="ghost"
                    onClick={() => void api.dismissNotification(n.id).then(refresh)}
                  >
                    Dismiss
                  </Button>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
