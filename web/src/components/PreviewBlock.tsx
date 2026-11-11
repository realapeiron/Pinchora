import type { ReactNode } from "react";
import { MessagePreview } from "./MessagePreview";

export function PreviewBlock({
  label,
  text,
  children,
}: {
  label: string;
  text?: string;
  children?: ReactNode;
}) {
  return (
    <div className="settings-preview">
      <span className="field-label">{label}</span>
      <MessagePreview
        preview={{ text: text ?? "", embed: null, ephemeral: false, notes: [] }}
        label={label}
      />
      {children}
    </div>
  );
}
