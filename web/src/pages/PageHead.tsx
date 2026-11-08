import type { ReactNode } from "react";
import { translate } from "../i18n";
import { PageTitleContext } from "../components/PageTitleContext";

interface PageHeadProps {
  title: string;
  lead: ReactNode;
  action?: ReactNode;
}

/** One page title, with distinct subsection headings supplied by its panels. */
export function SettingsPage({
  title,
  lead,
  action,
  className = "settings-page",
  children,
}: PageHeadProps & { className?: string; children: ReactNode }) {
  return (
    <PageTitleContext value={translate(title)}>
      <div className={className}>
        <PageHead title={title} lead={lead} action={action} />
        {children}
      </div>
    </PageTitleContext>
  );
}

/** A settings page's title, the line under it, and an optional action on the right. */
export function PageHead({ title, lead, action }: PageHeadProps) {
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
