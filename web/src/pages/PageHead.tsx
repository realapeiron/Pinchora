import type { ReactNode } from "react";
import { translate } from "../i18n";

/** A settings page's title, the line under it, and an optional action on the right. */
export function PageHead({ title, lead, action }: { title: string; lead: ReactNode; action?: ReactNode }) {
  return (
    <div className="page-head">
      <div>
        <h1 className="pixel-heading">{translate(title)}</h1>
        <p className="muted">{typeof lead === "string" ? translate(lead) : lead}</p>
      </div>
      {action}
    </div>
  );
}
