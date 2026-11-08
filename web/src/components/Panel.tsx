import { useContext, type ReactNode } from "react";
import { locale, translate } from "../i18n";
import { PageTitleContext } from "./PageTitleContext";

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
  const pageTitle = useContext(PageTitleContext);
  const heading = title ? translate(title) : undefined;
  const normalize = (value: string) => value.trim().toLocaleLowerCase(locale());
  const showHeading = heading && (!pageTitle || normalize(heading) !== normalize(pageTitle));
  return (
    <section className={`panel panel-${tone}`}>
      <div className="panel-body">
        {(showHeading || aside) && (
          <header className="panel-head">
            {showHeading && <h2 className="pixel-title">{heading}</h2>}
            {aside}
          </header>
        )}
        {children}
      </div>
    </section>
  );
}
