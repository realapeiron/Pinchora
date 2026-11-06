import type { ReactNode } from "react";

/** A settings page's title, the line under it, and an optional action on the right. */
export function PageHead({ title, lead, action }: { title: string; lead: ReactNode; action?: ReactNode }) {
  return (
    <div className="page-head">
      <div>
        <h1 className="pixel-heading">{title}</h1>
        <p className="muted">{lead}</p>
      </div>
      {action}
    </div>
  );
}
