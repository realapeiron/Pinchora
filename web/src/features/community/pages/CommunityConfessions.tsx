import { useState } from "react";
import { translate as t, dateTime } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ChannelPicker } from "../../../components/Pickers";
import { State, useTools, useWrite, type Data } from "./communityToolsShared";
import { useDiscord } from "../../../lib/queries";
import { communitySummary, confessionSettingsSummary } from "./communitySummary";

export function CommunityConfessions() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  if (!query.data)
    return (
      <SettingsPage
        className="settings-page community-tools-page"
        title={t("ANONYMOUS CONFESSIONS")}
        lead={t("Members submit anonymously; staff review before public posting.")}
      >
        <State query={query} />
      </SettingsPage>
    );
  return (
    <ConfessionForm
      key={guild + JSON.stringify(query.data.confession_settings)}
      data={query.data}
      discord={discord.data}
      busy={busy}
      notice={notice}
      write={write}
    />
  );
}
function ConfessionForm({
  data,
  discord,
  busy,
  notice,
  write,
}: {
  data: Data;
  discord: ReturnType<typeof useDiscord>["data"];
  busy: boolean;
  notice: string;
  write: ReturnType<typeof useWrite>["write"];
}) {
  const [review, setReview] = useState<string | null>(data.confession_settings.review_channel_id);
  const [publish, setPublish] = useState<string | null>(data.confession_settings.publish_channel_id);
  const [enabled, setEnabled] = useState(data.confession_settings.enabled);
  const changed =
    review !== data.confession_settings.review_channel_id ||
    publish !== data.confession_settings.publish_channel_id ||
    enabled !== data.confession_settings.enabled;
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("ANONYMOUS CONFESSIONS")}
      lead={t("Members submit anonymously; staff review before public posting.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <div className="settings-detail-list">
        <SettingsRow
          rowId="community-confessions-review-settings"
          title={dt("Review settings", {}, "REVIEW SETTINGS")}
          description={dt("Choose staff and public channels for confessions.")}
          summary={confessionSettingsSummary(enabled)}
          initialOpen
          enabled={enabled}
          onToggle={setEnabled}
          toggleLabel={t("Enabled")}
        >
          <div className="community-v2-controls">
            <label>
              {t("Staff review channel")}
              <ChannelPicker info={discord} value={review} onChange={setReview} />
            </label>
            <label>
              {t("Public channel")}
              <ChannelPicker info={discord} value={publish} onChange={setPublish} />
            </label>
            <button
              className="pixel-button"
              disabled={!changed || busy || !review || !publish || review === publish}
              onClick={() =>
                write("/confessions/settings", "PUT", {
                  review_channel_id: review,
                  publish_channel_id: publish,
                  enabled,
                })
              }
            >
              {t("Save")}
            </button>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="community-confessions-review-queue"
          title={dt("Review queue", {}, "REVIEW QUEUE")}
          description={dt("Approve or deny pending confessions.")}
          summary={communitySummary(data).queue}
        >
          {data.confessions.filter((c) => c.status === "pending").length === 0 ? (
            <p className="muted">{t("Queue is clear.")}</p>
          ) : (
            <div className="community-v2-list">
              {data.confessions
                .filter((c) => c.status === "pending")
                .map((c) => (
                  <article className="community-v2-item" key={c.id}>
                    <strong>
                      #{c.id} · {dateTime(c.created_at)}
                    </strong>
                    <p>{c.body}</p>
                    <div className="community-v2-actions">
                      <button
                        className="pixel-button"
                        disabled={busy}
                        onClick={() => void write(`/confessions/${c.id}/review`, "PUT", { approve: true })}
                      >
                        {t("Approve")}
                      </button>
                      <button
                        className="pixel-button"
                        disabled={busy}
                        onClick={() => void write(`/confessions/${c.id}/review`, "PUT", { approve: false })}
                      >
                        {t("Deny")}
                      </button>
                    </div>
                  </article>
                ))}
            </div>
          )}
        </SettingsRow>
        <SettingsRow
          rowId="community-confessions-reported-confessions"
          title={dt("Reported confessions", {}, "REPORTED CONFESSIONS")}
          description={dt("Review reports and block authors.")}
          summary={communitySummary(data).reports}
        >
          <div className="community-v2-list">
            {data.confessions
              .filter((c) => c.reports > 0)
              .map((c) => (
                <article className="community-v2-item" key={c.id}>
                  <strong>
                    #{c.id} · {t("Reports")}: {c.reports}
                  </strong>
                  <p>{c.body}</p>
                  <div className="community-v2-actions">
                    <button
                      className="pixel-button"
                      disabled={busy}
                      onClick={() => void write(`/confessions/${c.id}/ban`, "PUT", { banned: true })}
                    >
                      {t("Block author")}
                    </button>
                    <button
                      className="pixel-button"
                      disabled={busy}
                      onClick={() => void write(`/confessions/${c.id}/ban`, "PUT", { banned: false })}
                    >
                      {t("Unblock author")}
                    </button>
                  </div>
                </article>
              ))}
          </div>
        </SettingsRow>
      </div>
    </SettingsPage>
  );
}
