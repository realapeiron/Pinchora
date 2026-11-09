import { useEffect } from "react";
import { useBlocker } from "react-router";
import { PixelButton } from "../../../components/PixelButton";
import type { words } from "../api/verification";
/** Safety's catalog-backed variant of the dashboard save bar. */
export function SafetySaveBar({
  dirty,
  saving,
  t,
  onSave,
  onReset,
}: {
  dirty: boolean;
  saving: boolean;
  t: ReturnType<typeof words>;
  onSave: () => void;
  onReset: () => void;
}) {
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) => dirty && currentLocation.pathname !== nextLocation.pathname,
  );
  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => e.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);
  return (
    <>
      <div
        className={`save-bar ${dirty ? "is-open" : ""}`}
        role="region"
        aria-label={t("dirty")}
        inert={!dirty}
      >
        <div className="save-bar-inner">
          <span className="save-bar-count">{t("dirty")}</span>
          <div className="save-bar-actions">
            <PixelButton variant="ghost" onClick={onReset} disabled={saving}>
              {t("reset")}
            </PixelButton>
            <PixelButton onClick={onSave} disabled={saving}>
              {t(saving ? "saving" : "save")}
            </PixelButton>
          </div>
        </div>
      </div>
      {blocker.state === "blocked" && (
        <div className="dialog-backdrop">
          <div className="panel dialog" role="alertdialog" aria-label={t("dirty")}>
            <div className="panel-body">
              <p>{t("dirty")}</p>
              <div className="dialog-actions">
                <PixelButton onClick={() => blocker.reset()}>{t("stay")}</PixelButton>
                <PixelButton variant="ghost" onClick={() => blocker.proceed()}>
                  {t("reset")}
                </PixelButton>
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
