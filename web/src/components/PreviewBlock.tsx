import type { ReactNode } from "react";
import { MessagePreview } from "./MessagePreview";

export function PreviewBlock({
  label,
  text,
  status,
  children,
}: {
  label: string;
  text?: string;
  status?: string;
  children?: ReactNode;
}) {
  return (
    <div className="settings-preview">
      <span className="field-label">{label}</span>
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
