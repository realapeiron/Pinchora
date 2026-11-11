import { useId, useState, type ReactNode } from "react";
import { PixelToggle } from "./PixelToggle";

const openRows = new Map<string, boolean>();

function savedOpen(key: string, initialOpen: boolean): boolean {
  const remembered = openRows.get(key);
  if (remembered !== undefined) return remembered;
  try {
    const saved = window.sessionStorage.getItem(key);
    if (saved !== null) return saved === "true";
  } catch {
    // Storage can be unavailable in private browsing or blocked contexts.
  }
  return initialOpen;
}

function rememberOpen(key: string, open: boolean): void {
  openRows.set(key, open);
  try {
    window.sessionStorage.setItem(key, String(open));
  } catch {
    // The in-memory value still survives component remounts.
  }
}

export function SettingsRow({
  rowId,
  title,
  description,
  summary,
  enabled,
  onToggle,
  toggleLabel,
  toggleDisabled,
  children,
  initialOpen = false,
}: {
  rowId: string;
  title: string;
  description: string;
  summary: string;
  enabled?: boolean;
  onToggle?: (next: boolean) => void;
  toggleLabel?: string;
  toggleDisabled?: boolean;
  children: ReactNode;
  initialOpen?: boolean;
}) {
  const storageKey = `settings-row:${typeof window === "undefined" ? "" : window.location.pathname}:${rowId}`;
  const [open, setOpen] = useState(() =>
    typeof window === "undefined" ? initialOpen : savedOpen(storageKey, initialOpen),
  );
  const toggleOpen = () => {
    const next = !open;
    rememberOpen(storageKey, next);
    setOpen(next);
  };
  const bodyId = useId();
  return (
    <section className={`settings-detail-row ${open ? "is-open" : ""}`}>
      <div className="settings-detail-head">
        <button
          type="button"
          className="settings-detail-expand"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={toggleOpen}
        >
          <span className="settings-detail-chevron" aria-hidden="true">
            ›
          </span>
          <span className="settings-detail-copy">
            <strong>{title}</strong>
            <span
              className={open ? "settings-detail-description" : "settings-detail-summary"}
              title={open ? description : summary}
            >
              {open ? description : summary}
            </span>
          </span>
        </button>
        {onToggle && enabled !== undefined && (
          <PixelToggle
            label={toggleLabel ?? title}
            checked={enabled}
            onChange={onToggle}
            disabled={toggleDisabled}
          />
        )}
      </div>
      <div id={bodyId} className="settings-detail-body" hidden={!open}>
        {children}
      </div>
    </section>
  );
}

export function FieldGrid({ children }: { children: ReactNode }) {
  return <div className="settings-field-grid">{children}</div>;
}
