import { useState } from "react";
import { api } from "../../lib/api";
import { t } from "../i18n";
import type { RunAction } from "../SpacesPanel";

export type Note = { id: string; name: string; body: string; author_id: string };

export function Guestbook({
  plaza,
  owner,
  user,
  busy,
  run,
  notes,
}: {
  plaza: boolean;
  owner: string;
  user: string;
  busy: boolean;
  run: RunAction;
  notes: Note[] | undefined;
}) {
  const [text, setText] = useState("");
  return (
    <>
      {!plaza && (
        <>
          <h3>{t("Guestbook")}</h3>
          {owner !== user && (
            <>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await api<{ appreciated: boolean }>(
                      `/api/reef/houses/${owner}/appreciate`,
                      "POST",
                      {},
                    );
                    if (!result.appreciated) throw Error();
                  }, "Appreciated")
                }
              >
                {t("Appreciate")}
              </button>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void run(async () => {
                    const result = await api<{ signed: boolean }>(
                      `/api/reef/houses/${owner}/guestbook`,
                      "POST",
                      { text },
                    );
                    if (!result.signed) throw Error();
                    setText("");
                  });
                }}
              >
                <label>
                  {t("Leave a note")}
                  <textarea value={text} maxLength={200} onChange={(e) => setText(e.target.value)} required />
                </label>
                <button disabled={busy || !text.trim()}>{t("Sign guestbook")}</button>
              </form>
            </>
          )}
          <ul className="reef-notes">
            {notes?.map((n) => (
              <li key={n.id}>
                <strong>{n.name}</strong>
                <p>{n.body}</p>
                <div className="reef-space-actions">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void run(
                        () =>
                          api(`/api/reef/houses/${owner}/guestbook/${n.id}/report`, "POST", {
                            reason: "other",
                          }),
                        "Report received",
                      )
                    }
                  >
                    {t("Report")}
                  </button>
                  {n.author_id !== user && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void run(() => api(`/api/reef/blocks/${n.author_id}`, "PUT", {}))}
                    >
                      {t("Block")}
                    </button>
                  )}
                  {owner === user && (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void run(() => api(`/api/me/guestbook/${n.id}`, "DELETE"))}
                    >
                      {t("Delete")}
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}
