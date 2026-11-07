import type { ReactNode } from "react";
import { translate } from "../i18n";

/** The cards' notched panel: a coloured outline around a dark fill, corners cut by one step. */
export function Panel({
  title,
  aside,
  tone = "edge",
  children,
}: {
  title?: string;
  aside?: ReactNode;
  tone?: "edge" | "accent" | "gold";
  children: ReactNode;
}) {
  return (
    <section className={`panel panel-${tone}`}>
      <div className="panel-body">
        {(title || aside) && (
          <header className="panel-head">
            {title && <h2 className="pixel-title">{translate(title)}</h2>}
            {aside}
          </header>
        )}
        {children}
      </div>
    </section>
  );
}
