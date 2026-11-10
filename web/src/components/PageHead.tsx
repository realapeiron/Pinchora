import { useContext, type ReactNode } from "react";
import { translate } from "../i18n";
import { PageTitleContext } from "./PageTitleContext";
import { GuildPageContext } from "./GuildPageContext";

interface PageHeadProps {
  title: string;
  lead: ReactNode;
  action?: ReactNode;
  eyebrow?: string;
}

export function DisplayText({ children }: { children: string }) {
  if (!children.includes("&")) return children;
  return (
    <>
      {children.split("&").map((piece, index) => (
        <span key={index}>
          {index > 0 && <span className="display-amp">&amp;</span>}
          {piece}
        </span>
      ))}
    </>
  );
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
export function PageHead({ title, lead, action, eyebrow }: PageHeadProps) {
  const route = useContext(GuildPageContext);
  return (
    <div className="page-head">
      {route && (
        <span className="page-head-icon">
          <svg className="pixel-icon" aria-hidden="true" focusable="false">
            <use href={`#px-${route.icon}`} />
          </svg>
        </span>
      )}
      <div className="page-head-copy">
        {(eyebrow || route) && <span className="page-eyebrow">{translate(eyebrow ?? route!.group)}</span>}
        <h1 className="pixel-heading">
          <DisplayText>{translate(title)}</DisplayText>
        </h1>
        <p className="muted">{typeof lead === "string" ? translate(lead) : lead}</p>
      </div>
      {action}
    </div>
  );
}
