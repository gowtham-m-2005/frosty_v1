import { useState } from "react";
import { Banner, Button, PermissionSheet, SettingsForm, Toggle, type JsonSchema } from "@frosty/ui";

import { api, type CapabilityInfo } from "../bridge";
import { useCapabilities } from "../capabilities";

const STATE_LABEL: Record<CapabilityInfo["state"], string> = {
  enabled: "On",
  enabling: "Turning on…",
  disabled: "Off",
  error: "Error",
};

function Settings({ info }: { info: CapabilityInfo }) {
  const [loaded, setLoaded] = useState<{
    schema: JsonSchema;
    values: Record<string, unknown>;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!loaded && !error) {
    void Promise.all([api.getCapabilitySchema(info.id), api.getCapabilitySettings(info.id)]).then(
      ([schema, values]) => setLoaded({ schema: (schema ?? {}) as JsonSchema, values }),
      (e: unknown) => setError(String(e)),
    );
  }
  if (error) return <Banner>{error}</Banner>;
  if (!loaded) return <p className="muted">Loading settings…</p>;
  return (
    <SettingsForm
      schema={loaded.schema}
      values={loaded.values}
      onSave={async (changes) => {
        await api.setCapabilitySettings(info.id, changes);
        setLoaded({ ...loaded, values: { ...loaded.values, ...changes } });
      }}
    />
  );
}

function Row({
  info,
  onAskEnable,
}: {
  info: CapabilityInfo;
  onAskEnable: (info: CapabilityInfo) => void;
}) {
  const [showSettings, setShowSettings] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [deleteData, setDeleteData] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const on = info.state === "enabled" || info.state === "enabling";

  const run = (fn: () => Promise<unknown>) => fn().catch((e: unknown) => setProblem(String(e)));

  return (
    <li className="module-row">
      <div className="module-head">
        <div>
          <strong>{info.name}</strong> <span className="muted">v{info.version}</span>
          <div className="muted">{info.description}</div>
          <div className="module-tags">
            <span className={`tag tag-${info.state}`}>{STATE_LABEL[info.state]}</span>
            <span className="tag">{info.origin === "built_in" ? "Built-in" : "Installed"}</span>
            <span className="tag">{info.runtime === "ts" ? "TypeScript" : info.runtime}</span>
          </div>
        </div>
        <div className="module-actions">
          <Toggle
            label={`Turn ${info.name} ${on ? "off" : "on"}`}
            checked={on}
            onChange={(next) => {
              if (next) onAskEnable(info);
              else void run(() => api.disableCapability(info.id));
            }}
          />
          {info.has_settings ? (
            <Button onClick={() => setShowSettings((s) => !s)} aria-expanded={showSettings}>
              Settings
            </Button>
          ) : null}
          <details className="module-menu">
            <summary aria-label={`More actions for ${info.name}`}>⋯</summary>
            <div>
              <button
                type="button"
                onClick={() => void run(() => api.capabilityReveal(info.id, "config"))}
              >
                Open config file
              </button>
              <button
                type="button"
                onClick={() => void run(() => api.capabilityReveal(info.id, "folder"))}
              >
                Reveal folder
              </button>
              <button
                type="button"
                disabled={info.origin === "built_in" || on}
                title={
                  info.origin === "built_in"
                    ? "Built-in modules can be turned off but not removed"
                    : on
                      ? "Turn it off first"
                      : undefined
                }
                onClick={() => setRemoving(true)}
              >
                Remove…
              </button>
            </div>
          </details>
        </div>
      </div>

      {info.state === "error" && info.error ? (
        <div className="module-error">
          <Banner>{info.error}</Banner>
          <Button
            onClick={() =>
              void navigator.clipboard?.writeText(`${info.id} ${info.version}\n${info.error}`)
            }
          >
            Copy details
          </Button>
        </div>
      ) : null}
      {problem ? <Banner>{problem}</Banner> : null}

      {removing ? (
        <div className="module-remove">
          <p>Remove {info.name} from this computer?</p>
          <label>
            <input
              type="checkbox"
              checked={deleteData}
              onChange={(e) => setDeleteData(e.target.checked)}
            />{" "}
            Also delete its data
          </label>
          <div className="f-form-actions">
            <Button onClick={() => setRemoving(false)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={() =>
                void run(async () => {
                  await api.removeCapability(info.id, deleteData);
                  setRemoving(false);
                })
              }
            >
              Remove
            </Button>
          </div>
        </div>
      ) : null}

      {showSettings ? (
        <div className="module-settings">
          <Settings info={info} />
        </div>
      ) : null}
    </li>
  );
}

export function ModulesPage() {
  const { capabilities, rejected } = useCapabilities();
  const [asking, setAsking] = useState<CapabilityInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  const grant = (confirmedShell: boolean) => {
    if (!asking) return;
    setBusy(true);
    api
      .enableCapability(asking.id, confirmedShell)
      .then(() => setAsking(null))
      .catch((e: unknown) => setProblem(String(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="modules">
      <div className="modules-toolbar">
        <Button
          variant="primary"
          onClick={() =>
            void api.installCapabilityFromFolder().catch((e: unknown) => setProblem(String(e)))
          }
        >
          Install from folder…
        </Button>
        <Button onClick={() => void api.rescanCapabilities()}>Rescan</Button>
      </div>

      {problem ? <Banner>{problem}</Banner> : null}

      {capabilities.length === 0 ? (
        <section className="empty-state">
          <h2>No modules found</h2>
          <p>
            Built-in modules appear here once they are built. Install one from a folder to add more.
          </p>
        </section>
      ) : (
        <ul className="module-list">
          {capabilities.map((c) => (
            <Row key={c.id} info={c} onAskEnable={setAsking} />
          ))}
        </ul>
      )}

      {rejected.length > 0 ? (
        <section>
          <h2>Modules that could not be loaded</h2>
          {rejected.map((r) => (
            <Banner key={r.dir}>
              <code>{r.dir}</code>
              <ul>
                {r.errors.map((e) => (
                  <li key={e}>{e}</li>
                ))}
              </ul>
            </Banner>
          ))}
        </section>
      ) : null}

      {asking ? (
        <PermissionSheet
          name={asking.name}
          lines={asking.permissions}
          busy={busy}
          onGrant={grant}
          onCancel={() => setAsking(null)}
        />
      ) : null}
    </div>
  );
}
