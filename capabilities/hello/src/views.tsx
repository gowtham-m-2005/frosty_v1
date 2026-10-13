import { useState } from "react";
import { useEvent, useSettings, useTool, useToolData } from "@frosty/sdk/views";
import { defineViews } from "@frosty/sdk";
import type { Row } from "@frosty/sdk";

function Greeter({ compact }: { compact: boolean }) {
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const greet = useTool<{ name: string }, { id: string; text: string }>("hello.greet");
  const clear = useTool("hello.clear");
  const settings = useSettings<{ accent?: string }>();
  const { data, loading, error, refresh } = useToolData<Row[]>("hello.list", {}, ["hello.greeted"]);

  useEvent("hello.greeted", () => refresh());

  const submit = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setProblem(null);
    greet({ name: trimmed }).then(
      () => setName(""),
      (e: unknown) => setProblem(e instanceof Error ? e.message : String(e)),
    );
  };

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          className="f-input"
          aria-label="Who to greet"
          placeholder="Name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          style={{ flex: 1 }}
        />
        <button
          type="button"
          className="f-button f-button-primary"
          onClick={submit}
          style={settings?.accent ? { background: settings.accent } : undefined}
        >
          Greet
        </button>
      </div>

      {problem ? <p role="alert">{problem}</p> : null}
      {error ? <p role="alert">Could not load greetings: {error}</p> : null}
      {loading && !data ? <p className="muted">Loading…</p> : null}
      {data && data.length === 0 ? (
        <p className="muted">No greetings yet. Say hello to someone.</p>
      ) : null}

      <ul style={{ margin: 0, paddingLeft: 18 }}>
        {(data ?? []).slice(0, compact ? 5 : 20).map((row) => (
          <li key={row.id}>{String(row.text)}</li>
        ))}
      </ul>

      {data && data.length > 0 ? (
        <button
          type="button"
          className="f-button f-button-ghost"
          onClick={() => void clear().then(refresh)}
        >
          Clear all
        </button>
      ) : null}
    </div>
  );
}

export default defineViews({
  card: () => <Greeter compact />,
  page: () => <Greeter compact={false} />,
});
