import { useEffect, useRef, useState } from "react";

import { Button } from "./components";

export interface PermissionLine {
  key: string;
  text: string;
  sensitive: boolean;
}

/**
 * Lists exactly what a module asks for, in plain language, before it is
 * enabled for the first time (docs/DESIGN_SPEC.md 4.3). A request to run
 * programs needs a second, separate confirmation.
 */
export function PermissionSheet({
  name,
  lines,
  busy,
  onGrant,
  onCancel,
}: {
  name: string;
  lines: PermissionLine[];
  busy?: boolean;
  onGrant: (confirmedShell: boolean) => void;
  onCancel: () => void;
}) {
  const needsShell = lines.some((l) => l.key === "shell");
  const [understood, setUnderstood] = useState(false);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="f-scrim">
      <div role="dialog" aria-modal="true" aria-labelledby="perm-title" className="f-sheet">
        <h2 id="perm-title">{name} wants to:</h2>
        {lines.length === 0 ? (
          <p>It does not ask for any permissions.</p>
        ) : (
          <ul className="f-perm-list">
            {lines.map((line) => (
              <li key={line.key} className={line.sensitive ? "f-perm-sensitive" : undefined}>
                {line.sensitive ? <strong>Important: </strong> : null}
                {line.text}
              </li>
            ))}
          </ul>
        )}
        {needsShell ? (
          <label className="f-perm-confirm">
            <input
              type="checkbox"
              checked={understood}
              onChange={(e) => setUnderstood(e.target.checked)}
            />{" "}
            I understand this module runs as a separate program with the same access I have, and I
            trust where it came from.
          </label>
        ) : null}
        <div className="f-form-actions">
          <Button ref={cancelRef} onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant="primary"
            disabled={busy || (needsShell && !understood)}
            onClick={() => onGrant(understood)}
          >
            Allow and turn on
          </Button>
        </div>
      </div>
    </div>
  );
}
