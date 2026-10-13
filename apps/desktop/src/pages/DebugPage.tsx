import { useState } from "react";
import { Banner, Button } from "@frosty/ui";

import { api } from "../bridge";
import { useDecisionLog } from "../hooks";

interface Canned {
  label: string;
  name: string;
  payload?: Record<string, unknown>;
  significance?: number;
}

const CANNED: Canned[] = [
  { label: "First summon today", name: "core.first_summon_today" },
  { label: "Task created", name: "tasks.created" },
  { label: "Task completed", name: "tasks.completed" },
  { label: "Task completed (big)", name: "tasks.completed", significance: 0.95 },
  { label: "Reminder due", name: "tasks.due", payload: { title: "stretch" } },
  { label: "Streak milestone", name: "tasks.streak_milestone", payload: { count: 7 } },
  { label: "Setting saved", name: "core.setting_saved" },
  { label: "Empty state", name: "core.empty_state" },
  { label: "Do not disturb on", name: "core.dnd_enabled" },
  { label: "Error", name: "core.error" },
  { label: "Onboarding finished", name: "core.onboarding_finished" },
  { label: "Quit", name: "core.quit" },
];

/** Fires fake events and shows what the personality engine decided and why. */
export function DebugPage() {
  const log = useDecisionLog();
  const [name, setName] = useState("tasks.completed");
  const [payload, setPayload] = useState("{}");
  const [significance, setSignificance] = useState("");
  const [error, setError] = useState<string | null>(null);

  const fire = (event: Canned) => {
    setError(null);
    api
      .fireEvent(event.name, event.payload ?? {}, event.significance)
      .catch((e) => setError(String(e)));
  };

  const fireCustom = () => {
    setError(null);
    let parsed: unknown;
    try {
      parsed = JSON.parse(payload);
    } catch {
      setError("The payload is not valid JSON.");
      return;
    }
    const s = significance.trim() === "" ? undefined : Number(significance);
    if (s !== undefined && !(s >= 0 && s <= 1)) {
      setError("Significance must be a number from 0 to 1.");
      return;
    }
    api.fireEvent(name, parsed, s).catch((e) => setError(String(e)));
  };

  return (
    <div className="debug">
      <section>
        <h2>Fire an event</h2>
        <p className="muted">
          Events go through the same path as real ones: cooldowns, reaction level, and tone apply.
        </p>
        <div className="debug-buttons">
          {CANNED.map((c) => (
            <Button key={c.label} onClick={() => fire(c)}>
              {c.label}
            </Button>
          ))}
        </div>

        <h3>Custom</h3>
        <div className="debug-custom">
          <input
            className="f-input"
            aria-label="Event name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className="f-input"
            aria-label="Payload JSON"
            value={payload}
            onChange={(e) => setPayload(e.target.value)}
          />
          <input
            className="f-input"
            aria-label="Significance from 0 to 1"
            placeholder="significance"
            value={significance}
            onChange={(e) => setSignificance(e.target.value)}
          />
          <Button variant="primary" onClick={fireCustom}>
            Fire
          </Button>
        </div>
        {error ? <Banner>{error}</Banner> : null}
      </section>

      <section>
        <h2>Decisions</h2>
        {log.length === 0 ? <p className="muted">Nothing fired yet.</p> : null}
        <ol className="debug-log">
          {log.map((entry) => (
            <li key={`${entry.at}-${entry.event}`}>
              <code>{entry.event}</code>{" "}
              {entry.decision.kind === "play" ? (
                <span>
                  played <strong>{entry.decision.animation ?? "static chip"}</strong> at{" "}
                  {entry.decision.intensity.toFixed(2)}
                  {entry.decision.text ? <> saying “{entry.decision.text}”</> : null}
                </span>
              ) : (
                <span>dropped: {entry.decision.reason}</span>
              )}
            </li>
          ))}
        </ol>
      </section>
    </div>
  );
}
