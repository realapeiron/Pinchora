import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { translate as t } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { api } from "../../../lib/api";
import { Loading, Problem } from "../../../components/States";
import { State, useTools, type Personal } from "./communityToolsShared";
import { communitySummary, personalSummary } from "./communitySummary";

export function CommunityHighlights() {
  const { guild, query, client } = useTools();
  const personal = useQuery({
    queryKey: ["community-v2-personal", guild],
    queryFn: () => api<Personal>(`/api/guilds/${guild}/community-v2/personal`),
  });
  const [word, setWord] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  async function write(path: string, method: "PUT" | "DELETE", body?: unknown) {
    setBusy(true);
    setNotice("");
    try {
      await api(`/api/guilds/${guild}/community-v2/personal${path}`, method, body);
      await client.invalidateQueries({ queryKey: ["community-v2-personal", guild] });
      setNotice(t("Saved."));
      return true;
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not save."));
      return false;
    } finally {
      setBusy(false);
    }
  }
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("KEYWORD HIGHLIGHTS")}
      lead={t("Members opt in with /highlight add. A matching message sends a private link, never its text.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <State query={query} />
      <div className="settings-detail-list">
        {query.data && (
          <SettingsRow
            rowId="community-highlights-availability"
            title={dt("row.Availability", {}, "AVAILABILITY")}
            description={dt("Check whether keyword alerts can run.")}
            summary={communitySummary(query.data).availability}
            initialOpen
          >
            <p role="status">
              {query.data.message_content
                ? t("Message Content is enabled. Highlights are available.")
                : t("Needs Message Content intent. Highlights are locked until it is enabled.")}
            </p>
            <p className="muted">
              {t(
                "Each member can follow 10 words. Notifications are limited to one per 10 minutes per server.",
              )}
            </p>
          </SettingsRow>
        )}
        {personal.isPending && <Loading label={t("Loading followed words")} />}
        {personal.isError && <Problem error={personal.error} />}
        {personal.data && (
          <SettingsRow
            rowId="community-highlights-words-you-follow"
            title={dt("Words you follow", {}, "WORDS YOU FOLLOW")}
            description={dt("Manage your private keyword alerts.")}
            summary={personalSummary(personal.data).highlights}
          >
            <div className="community-v2-list">
              {personal.data.highlights.map((w) => (
                <div className="community-v2-item" key={w}>
                  <strong>{w}</strong>
                  <button
                    className="pixel-button"
                    disabled={busy}
                    onClick={() => void write(`/highlights/${encodeURIComponent(w)}`, "DELETE")}
                  >
                    {t("Remove")}
                  </button>
                </div>
              ))}
            </div>
            <div className="community-v2-controls">
              <label>
                {t("Keyword")}
                <input
                  className="pixel-input"
                  maxLength={40}
                  value={word}
                  onChange={(e) => setWord(e.target.value)}
                />
              </label>
              <button
                className="pixel-button"
                disabled={
                  !query.data?.message_content ||
                  !/^[\p{L}\p{N}]{2,40}$/u.test(word.trim()) ||
                  busy ||
                  personal.data.highlights.length >= 10 ||
                  personal.data.highlights.includes(word.trim().toLowerCase())
                }
                onClick={() =>
                  void write("/highlights", "PUT", { word }).then((saved) => saved && setWord(""))
                }
              >
                {t("Follow word")}
              </button>
            </div>
          </SettingsRow>
        )}
      </div>
    </SettingsPage>
  );
}
