import { useId, useState, type ReactNode } from "react";
import { PixelToggle } from "./PixelToggle";

export function SettingsRow({
  title,
  description,
  summary,
  enabled,
  onToggle,
  children,
  initialOpen = false,
}: {
  title: string;
  description: string;
  summary: string;
  enabled?: boolean;
  onToggle?: (next: boolean) => void;
  children: ReactNode;
  initialOpen?: boolean;
}) {
  const [open, setOpen] = useState(initialOpen);
  const bodyId = useId();
  return (
    <section className={`settings-detail-row ${open ? "is-open" : ""}`}>
      <div className="settings-detail-head">
        <button
          type="button"
          className="settings-detail-expand"
          aria-expanded={open}
          aria-controls={bodyId}
          onClick={() => setOpen(!open)}
        >
          <span className="settings-detail-chevron" aria-hidden="true">
            ›
          </span>
          <span className="settings-detail-copy">
            <strong>{title}</strong>
            <span className={open ? "settings-detail-description" : "settings-detail-summary"}>
              {open ? description : summary}
            </span>
          </span>
        </button>
        {onToggle && enabled !== undefined && (
          <PixelToggle label={title} checked={enabled} onChange={onToggle} />
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
