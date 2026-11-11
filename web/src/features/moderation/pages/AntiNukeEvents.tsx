import { dateTime, number } from "../../../i18n";
import { PixelButton } from "../../../components/PixelButton";
import { SettingsRow } from "../../../components/SettingsRow";
import type { AntiNukeData, useAntiNuke, words } from "../api/anti_nuke";

type Text = ReturnType<typeof words>;
type Actions = ReturnType<typeof useAntiNuke>;
export function AntiNukeEvents({
  saved,
  t,
  summary,
  restore,
  confirm,
  setConfirm,
  notice,
  setNotice,
}: {
  saved: AntiNukeData;
  t: Text;
  summary: string;
  restore: Actions["restore"];
  confirm: string | null;
  setConfirm: (value: string | null) => void;
  notice: string | null;
  setNotice: (value: string | null) => void;
}) {
  return (
    <SettingsRow
      rowId="anti-nuke-events-events"
      title={t("ui.events")}
      description={t("ui.events_hint")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="muted">{t("ui.events_hint")}</p>
        {saved.events.length === 0 ? (
          <p className="empty-row muted">{t("empty")}</p>
        ) : (
          <ol className="antinuke-events">
            {saved.events.map((event) => (
              <li key={event.id}>
                <div className="antinuke-event-head">
                  <h3>
                    {t(`kind.${event.kind}`)} <span className="muted">×{number(event.count)}</span>
                  </h3>
                  <time dateTime={event.created_at}>{dateTime(event.created_at)}</time>
                </div>
                <p>
                  <span className="muted">{t("ui.actor")}: </span>
                  <code>{event.actor_id}</code>
                </p>
                <p>{event.action}</p>
                {event.reverted ? (
                  <p className="antinuke-recovered">{t("ui.restored")}</p>
                ) : (
                  event.restorable &&
                  saved.can_restore && (
                    <PixelButton
                      variant="secondary"
                      disabled={restore.isPending}
                      onClick={() => setConfirm(event.id)}
                    >
                      {t("undo")}
                    </PixelButton>
                  )
                )}
                {confirm === event.id && (
                  <div className="antinuke-confirm" role="group" aria-label={t("ui.confirm")}>
                    <p>{t("ui.confirm_hint")}</p>
                    <div className="antinuke-confirm-actions">
                      <PixelButton
                        disabled={restore.isPending}
                        onClick={() =>
                          restore.mutate(event.id, {
                            onSuccess: (data) => {
                              setNotice(data.message);
                              setConfirm(null);
                            },
                          })
                        }
                      >
                        {t("ui.confirm")}
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        disabled={restore.isPending}
                        onClick={() => setConfirm(null)}
                      >
                        {t("ui.cancel")}
                      </PixelButton>
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ol>
        )}
        {notice && (
          <p role="status" className="notice">
            {notice}
          </p>
        )}
        {restore.isError && (
          <p className="field-error" role="alert">
            {restore.error.message}
          </p>
        )}
      </div>
    </SettingsRow>
  );
}
