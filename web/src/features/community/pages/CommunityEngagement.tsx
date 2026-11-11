import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useOutletContext } from "react-router";
import { translate as t } from "../../../i18n";
import { engagementSummary } from "./engagementSummary";
import { EngagementContext } from "./engagementContext";
import { QuestionSection } from "./QuestionSection";
import { CountingSection } from "./CountingSection";
import { ApplicationSection } from "./ApplicationSection";
import { ModmailSection } from "./ModmailSection";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { type Overview, api } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import {
  type ApplicationForm,
  type Counting,
  type MailSettings,
  type QotdSettings,
  useCommunityEngagement,
} from "../api/communityEngagement";
import "../../../styles/community-engagement.css";

const emptyQotd: QotdSettings = {
  channel_id: null,
  ping_role_id: null,
  hour: 12,
  timezone: "UTC",
  thread: false,
};
const emptyCounting: Counting = {
  channel_id: "",
  current: 0,
  record: 0,
  reset_on_fail: true,
  allow_math: false,
  fail_role_id: null,
};
const emptyMail: MailSettings = {
  enabled: false,
  forum_channel_id: null,
  staff_roles: [],
  greeting: "",
  close_after_hours: 72,
  anonymous_staff: false,
  log_channel_id: null,
};
export function CommunityEngagement() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useCommunityEngagement(guild);
  const discord = useDiscord(guild);
  const client = useQueryClient();
  const [qotd, setQotd] = useState<QotdSettings>(emptyQotd);
  const [counting, setCounting] = useState<Counting>(emptyCounting);
  const [mail, setMail] = useState<MailSettings>(emptyMail);
  const [form, setForm] = useState<ApplicationForm | null>(null);
  const [postChannel, setPostChannel] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    setQotd(query.data?.qotd ?? emptyQotd);
  }, [guild, query.data?.qotd]);
  // Query structural sharing keeps unchanged sections stable. Saving QOTD must not replace a
  // counting or modmail draft that the member edited while that request was in flight.
  useEffect(() => {
    setCounting(query.data?.counting ?? emptyCounting);
  }, [guild, query.data?.counting]);
  useEffect(() => {
    setMail(query.data?.modmail ?? emptyMail);
  }, [guild, query.data?.modmail]);
  async function write(path: string, method: "PUT" | "POST", body?: unknown) {
    setBusy(true);
    setNotice("");
    try {
      await api(`/api/guilds/${guild}/community-pack${path}`, method, body);
      await client.invalidateQueries({ queryKey: ["community-pack", guild] });
      setNotice(t("Saved community changes."));
      return true;
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not save changes."));
      return false;
    } finally {
      setBusy(false);
    }
  }
  const summary = query.data ? engagementSummary(query.data, qotd, counting, mail) : null;
  return (
    <SettingsPage
      className="settings-page community-pack-page"
      title={t("COMMUNITY PACK")}
      lead={t("Give members a daily question, a counting channel, applications and a private path to staff.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      {query.isPending && <Loading label={t("Loading community pack")} />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <>
          {!query.data.message_content && (
            <p className="notice" role="note">
              <strong className="badge badge-gold">{t("NEEDS MESSAGE CONTENT")}</strong>{" "}
              {t(
                "Counting reads the message text and staff replies from forum threads need the Message Content intent. Modmail DMs still arrive without it.",
              )}
            </p>
          )}
          <EngagementContext.Provider
            value={{
              data: query.data,
              info: discord.data,
              guild,
              qotd,
              setQotd,
              counting,
              setCounting,
              mail,
              setMail,
              form,
              setForm,
              postChannel,
              setPostChannel,
              busy,
              write,
              summary: summary!,
            }}
          >
            <div className="settings-detail-list community-pack-grid">
              <QuestionSection />
              <CountingSection />
              <ApplicationSection />
              <ModmailSection />
            </div>
          </EngagementContext.Provider>
          <p className="muted">
            {t("Members use")} <code>/qotd suggest</code>, <code>/counting stats</code> {t("and")}{" "}
            <code>/form list</code>. <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>{" "}
            {t("can turn each feature off.")}
          </p>
        </>
      )}
    </SettingsPage>
  );
}
