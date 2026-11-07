import type { ReactNode } from "react";

import { RichText } from "./RichText";
import { translate } from "../i18n";

/** A labelled form control with an optional hint and the problem with its value, if any. */
export function Field({
  label,
  htmlFor,
  hint,
  error,
  aside,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: ReactNode;
  error?: string | null;
  aside?: ReactNode;
  children: ReactNode;
}) {
  const hintId = htmlFor ? `${htmlFor}-hint` : undefined;
  return (
    <div className={`field ${error ? "has-error" : ""}`}>
      <div className="field-head">
        <label className="field-label" htmlFor={htmlFor}>
          {translate(label)}
        </label>
        {aside}
      </div>
      {children}
      {error ? (
        <p className="field-error" role="alert" id={hintId}>
          <RichText text={error} />
        </p>
      ) : (
        hint && (
          <p className="field-hint muted" id={hintId}>
            {hint}
          </p>
        )
      )}
    </div>
  );
}

/** A switch with its name and a line about what it does, as one settings row. */
export function SwitchRow({
  title,
  children,
  control,
}: {
  title: string;
  children?: ReactNode;
  control: ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-text">
        <h3 className="setting-name">{translate(title)}</h3>
        {children && <p className="muted">{children}</p>}
      </div>
      {control}
    </div>
  );
}
