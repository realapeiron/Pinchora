import type { ReactNode } from "react";
import { MessagePreview } from "./MessagePreview";

export function PreviewBlock({
  label,
  text,
  status,
  children,
  action,
}: {
  label: string;
  text?: string;
  status?: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="settings-preview">
      <div className="settings-preview-head">
        <span className="field-label">{label}</span>
        {action}
      </div>
      {status ? (
        <p className="muted" role="status">
          {status}
        </p>
      ) : (
        <MessagePreview
          preview={{ text: text ?? "", embed: null, ephemeral: false, notes: [] }}
          label={label}
        />
      )}
      {children}
    </div>
  );
}
