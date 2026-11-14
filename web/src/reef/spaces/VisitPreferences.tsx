import { useState } from "react";
import { api } from "../../lib/api";
import { t } from "../i18n";
import type { RunAction } from "../SpacesPanel";

export type Preferences = { presence: boolean; visits: string; doorbell: boolean };

export function VisitPreferences({
  busy,
  preferencesData,
  refetchPreferences,
  run,
}: {
  busy: boolean;
  preferencesData: Preferences | undefined;
  refetchPreferences: () => Promise<unknown>;
  run: RunAction;
}) {
  const [draft, setDraft] = useState<Preferences | null>(null);
  const p = draft ?? preferencesData;
  return (
    <>
      <details>
        <summary>{t("Visit preferences")}</summary>
        {p && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await api("/api/reef/preferences", "PUT", p);
                setDraft(null);
                await refetchPreferences();
              });
            }}
          >
            <label>
              <input
                type="checkbox"
                checked={p.presence}
                onChange={(e) => setDraft({ ...p, presence: e.target.checked })}
              />
              {t("Show online status to friends and partner")}
            </label>
            <label>
              <input
                type="checkbox"
                checked={p.doorbell}
                onChange={(e) => setDraft({ ...p, doorbell: e.target.checked })}
              />
              {t("Allow doorbells")}
            </label>
            <label>
              {t("Who can visit")}
              <select value={p.visits} onChange={(e) => setDraft({ ...p, visits: e.target.value })}>
                <option value="open">{t("Everyone")}</option>
                <option value="friends">{t("Friends and partner")}</option>
                <option value="doorbell">{t("Accepted doorbells")}</option>
                <option value="closed">{t("Nobody")}</option>
              </select>
            </label>
            <button disabled={busy || !draft || JSON.stringify(draft) === JSON.stringify(preferencesData)}>
              {t("Save")}
            </button>
          </form>
        )}
      </details>
    </>
  );
}
