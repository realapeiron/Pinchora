import { useEffect, useRef } from "react";
import { useBlocker } from "react-router";

import { unsavedLabel } from "../lib/draft";
import { PixelButton } from "./PixelButton";

/**
 * The sticky bar that appears while a page has unsaved edits: how many, Reset and Save. Leaving the
 * page (in the app or closing the tab) asks first.
 */
export function SaveBar({
  count,
  saving,
  error,
  blocked = false,
  onSave,
  onReset,
  words,
}: {
  count: number;
  saving: boolean;
  error?: string | null;
  /** Saving waits until the marked fields are fixed. */
  blocked?: boolean;
  onSave: () => void;
  onReset: () => void;
  words?: (key: string, values?: Record<string, string | number>) => string;
}) {
  const t =
    words ??
    ((key: string, values: Record<string, string | number> = {}) =>
      key.replace(/\{(\w+)\}/g, (match, name: string) => String(values[name] ?? match)));
  const changes = (n: number) =>
    words ? t(n === 1 ? "1 unsaved change" : "{count} unsaved changes", { count: n }) : unsavedLabel(n);
  const dirty = count > 0;
  // While the bar slides away after a save, it keeps showing what it said.
  const lastCount = useRef(count);
  if (dirty) lastCount.current = count;
  const blocker = useBlocker(({ currentLocation, nextLocation }) => {
    return dirty && currentLocation.pathname !== nextLocation.pathname;
  });

  useEffect(() => {
    if (!dirty) return;
    const warn = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  return (
    <>
      <div
        className={`save-bar ${dirty ? "is-open" : ""}`}
        role="region"
        aria-label={t("Unsaved changes")}
        inert={!dirty}
      >
        <div className="save-bar-inner">
          <span className="save-bar-count">{error ? error : changes(lastCount.current)}</span>
          <div className="save-bar-actions">
            <PixelButton variant="ghost" onClick={onReset} disabled={saving || !dirty}>
              {t("Reset")}
            </PixelButton>
            <PixelButton onClick={onSave} disabled={saving || !dirty || blocked}>
              {t(saving ? "Saving…" : "Save changes")}
            </PixelButton>
          </div>
        </div>
      </div>
      {blocker.state === "blocked" && (
        <div className="dialog-backdrop" role="presentation">
          <div className="panel panel-gold dialog" role="alertdialog" aria-labelledby="leave-title">
            <div className="panel-body">
              <h2 id="leave-title" className="pixel-title">
                {t("Unsaved changes")}
              </h2>
              <p>
                {t("You have {changes} on this page. Leave without saving?", { changes: changes(count) })}
              </p>
              <div className="dialog-actions">
                <PixelButton variant="secondary" onClick={() => blocker.reset()}>
                  {t("Stay")}
                </PixelButton>
                <PixelButton onClick={() => blocker.proceed()}>{t("Leave")}</PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
