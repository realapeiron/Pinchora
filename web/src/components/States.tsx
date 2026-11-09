import { translate as spaTranslate } from "../i18n";
import { ApiError } from "../lib/api";
import { Crab } from "./Crab";
import { Panel } from "./Panel";
import { t, translate } from "../i18n";

export function Loading({ full, label = spaTranslate("Loading") }: { full?: boolean; label?: string }) {
  return (
    <div className={full ? "state state-full" : "state"} role="status">
      <Crab scale={4} className="crab-walk" />
      <span className="pixel-label">{translate(label)}…</span>
    </div>
  );
}

export function Problem({ error, full }: { error: unknown; full?: boolean }) {
  const message =
    error instanceof ApiError ? error.message : t("Something went wrong on our side. Please try again.");
  return (
    <div className={full ? "state state-full" : "state"}>
      <Panel title={t("Hmm")} tone="accent">
        <p>{translate(message)}</p>
      </Panel>
    </div>
  );
}
