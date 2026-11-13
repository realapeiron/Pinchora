import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { translate as t } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { anniversarySummary } from "./communitySummary";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { State, useTools, useWrite, type Data } from "./communityToolsShared";
import { useDiscord } from "../../../lib/queries";

export function CommunityAnniversaries() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  if (!query.data)
    return (
      <SettingsPage
        className="settings-page community-tools-page"
        title={deferredText("MEMBER ANNIVERSARIES")}
        lead={deferredText("Celebrate when a member reaches another year in your server.")}
      >
        <State query={query} />
      </SettingsPage>
    );
  return (
    <AnniversaryForm
      key={guild + query.data.anniversary.channel_id + query.data.anniversary.enabled}
      data={query.data}
      discord={discord.data}
      busy={busy}
      notice={notice}
      write={write}
    />
  );
}
function AnniversaryForm({
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
  const [channel, setChannel] = useState<string | null>(data.anniversary.channel_id);
  const [enabled, setEnabled] = useState(data.anniversary.enabled);
  const changed = channel !== data.anniversary.channel_id || enabled !== data.anniversary.enabled;
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={deferredText("MEMBER ANNIVERSARIES")}
      lead={deferredText("Celebrate when a member reaches another year in your server.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <SettingsRow
        rowId="community-anniversaries-announcements"
        title={dt("Announcements", {}, "ANNOUNCEMENTS")}
        description={dt("Choose where yearly member milestones are posted.")}
        summary={anniversarySummary(channel, enabled)}
        initialOpen
        enabled={enabled}
        onToggle={setEnabled}
        toggleLabel={t("Enabled")}
      >
        <div className="community-v2-controls">
          <label>
            {t("Channel")}
            <ChannelPicker posting info={discord} value={channel} onChange={setChannel} />
          </label>
          <button
            className="pixel-button"
            disabled={!changed || busy || (enabled && !channel)}
            onClick={() => write("/anniversary", "PUT", { channel_id: channel, enabled })}
          >
            {t("Save")}
          </button>
        </div>
      </SettingsRow>
    </SettingsPage>
  );
}
