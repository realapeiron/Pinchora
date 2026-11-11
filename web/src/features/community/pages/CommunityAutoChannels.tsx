import { useState } from "react";
import { translate as t } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ChannelPicker } from "../../../components/Pickers";
import { State, useTools, useWrite } from "./communityToolsShared";
import { communitySummary } from "./communitySummary";

export function CommunityAutoChannels() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  const [channel, setChannel] = useState<string | null>(null);
  const [thread, setThread] = useState(false);
  const [crosspost, setCrosspost] = useState(false);
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("AUTO CHANNELS")}
      lead={t("Open discussion threads and crosspost announcements for new messages.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <State query={query} />
      {query.data && (
        <div className="settings-detail-list">
          <SettingsRow
            title={t("CHANNEL RULES")}
            description={dt("Review channels with automatic actions.")}
            summary={communitySummary(query.data).channels}
            initialOpen
          >
            {query.data.autopost.length === 0 ? (
              <p className="muted">{t("No channel rules yet.")}</p>
            ) : (
              query.data.autopost.map((c) => (
                <div className="community-v2-item" key={c.channel_id}>
                  <strong>#{c.channel_id}</strong>
                  <span className="muted">
                    {c.thread ? t("Auto thread") : ""} {c.crosspost ? t("Auto crosspost") : ""}
                  </span>
                  <button
                    className="pixel-button"
                    disabled={busy}
                    onClick={() => write(`/autopost/${c.channel_id}`, "DELETE")}
                  >
                    {t("Remove")}
                  </button>
                </div>
              ))
            )}
          </SettingsRow>
          <SettingsRow
            title={t("ADD CHANNEL")}
            description={dt("Choose a channel and its automatic actions.")}
            summary={channel ? t("Channel selected") : t("No channel")}
          >
            <div className="community-v2-controls">
              <label>
                {t("Channel")}
                <ChannelPicker info={discord.data} value={channel} onChange={setChannel} />
              </label>
              <label>
                <input type="checkbox" checked={thread} onChange={(e) => setThread(e.target.checked)} />{" "}
                {t("Auto thread")}
              </label>
              <label>
                <input type="checkbox" checked={crosspost} onChange={(e) => setCrosspost(e.target.checked)} />{" "}
                {t("Auto crosspost")}
              </label>
              <button
                className="pixel-button"
                disabled={
                  !channel ||
                  (!thread && !crosspost) ||
                  busy ||
                  query.data.autopost.some(
                    (c) => c.channel_id === channel && c.thread === thread && c.crosspost === crosspost,
                  )
                }
                onClick={() => write("/autopost", "PUT", { channel_id: channel, thread, crosspost })}
              >
                {t("Save")}
              </button>
            </div>
          </SettingsRow>
        </div>
      )}
    </SettingsPage>
  );
}
