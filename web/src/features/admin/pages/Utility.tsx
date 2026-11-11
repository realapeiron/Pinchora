import "../../../styles/utility.css";

import { useState } from "react";
import { dateTime } from "../../../i18n";
import { Link, useOutletContext } from "react-router";

import { MessagePreview } from "../../../components/MessagePreview";

import { type EmbedSpec, type Overview } from "../../../lib/api";
import { catalogWords } from "../api/catalogWords";
import { useDiscord } from "../../../lib/queries";
import { type ScheduledDraft, type Sticky, useUtility } from "../api/utility";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

import { UtilityWords } from "./utilityWords";
import { TimezoneEditor } from "./UtilityCards";
import { UtilityLists } from "./UtilityLists";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { utilitySummary } from "./utilitySummary";
import { StickyEditor } from "./StickyEditor";
import { ScheduleEditor } from "./ScheduleEditor";

export function Utility() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const utility = useUtility(guild);
  const {
    query,
    saveSticky,
    removeSticky,
    addScheduled,
    saveScheduled,
    removeScheduled,
    enableScheduled,
    previewDraft,
    saveTimezone,
  } = utility;
  const language = query.data?.language ?? "en";
  const t = catalogWords(query.data?.strings, language);
  const discord = useDiscord(guild);
  const [sticky, setSticky] = useState<Sticky | null>(null);
  const [scheduled, setScheduled] = useState<ScheduledDraft | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [previewItem, setPreviewItem] = useState<{ content: string; embed: EmbedSpec | null } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [runs, setRuns] = useState<string[]>([]);
  const module = data.modules.find((m) => m.key === "utility");
  const showError = (error: Error | null) =>
    error && (
      <p className="field-error" role="alert">
        {error.message}
      </p>
    );
  return (
    <UtilityWords.Provider value={{ t, language }}>
      <SettingsPage
        className="settings-page utility-page"
        title={t("MESSAGES")}
        lead={t(
          "Keep an important note visible, or send a message at the right time. AFK notices are set by each member with /afk set.",
        )}
      >
        {module && !module.enabled && (
          <p className="notice">
            <strong>{t("Messages are off.")}</strong>{" "}
            {t("Enable Messages to use sticky posts and AFK notices.")}{" "}
            <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>
          </p>
        )}
        {query.isPending && <Loading label={t("Reading messages")} />}
        {query.isError && <Problem error={query.error} />}
        {query.isSuccess && (
          <>
            <div className="utility-summary">
              <div>
                <span className="pixel-label">{t("STICKY")}</span>
                <strong>{query.data.sticky.length}</strong>
                <small>{t("channel notes")}</small>
              </div>
              <div>
                <span className="pixel-label">{t("SCHEDULED")}</span>
                <strong>
                  {query.data.scheduled.length}/{query.data.max_scheduled}
                </strong>
                <small>{t("used")}</small>
              </div>
              <div>
                <span className="pixel-label">{t("AFK")}</span>
                <strong>{t("5 min")}</strong>
                <small>{t("mention cooldown per channel")}</small>
              </div>
            </div>
            <div className="settings-detail-list">
              <SettingsRow
                title={t("SERVER TIME")}
                description={dashboardT("Set the server time zone for scheduled messages.")}
                summary={utilitySummary(query.data).timezone}
              >
                <TimezoneEditor
                  initial={query.data.timezone}
                  busy={saveTimezone.isPending}
                  error={saveTimezone.error}
                  onSave={(value) =>
                    saveTimezone.mutate(value, {
                      onSuccess: () => setNotice(t("Server time zone saved.")),
                      onError: (e) => setNotice(e.message),
                    })
                  }
                />
              </SettingsRow>
            </div>
            <UtilityLists
              data={query.data}
              info={discord.data}
              actions={utility}
              setSticky={setSticky}
              setScheduled={setScheduled}
              setEditingId={setEditingId}
              setNotice={setNotice}
              t={t}
            />
          </>
        )}
        {sticky && (
          <StickyEditor
            draft={sticky}
            info={discord.data}
            busy={saveSticky.isPending}
            error={saveSticky.error}
            onChange={setSticky}
            onPreview={() => {
              setPreviewItem(sticky);
              setRuns([]);
            }}
            onCancel={() => setSticky(null)}
            onSave={() =>
              saveSticky.mutate(sticky, {
                onSuccess: () => {
                  setSticky(null);
                  setNotice(t("Sticky message saved."));
                },
              })
            }
          />
        )}
        {scheduled && (
          <ScheduleEditor
            draft={scheduled}
            editing={editingId !== null}
            info={discord.data}
            busy={editingId === null ? addScheduled.isPending : saveScheduled.isPending}
            error={editingId === null ? addScheduled.error : saveScheduled.error}
            onChange={setScheduled}
            onPreview={() => {
              previewDraft.mutate(scheduled, {
                onSuccess: (result) => {
                  setPreviewItem(scheduled);
                  setRuns(result.runs);
                },
                onError: (e) => setNotice(e.message),
              });
            }}
            onCancel={() => {
              setScheduled(null);
              setEditingId(null);
            }}
            onSave={() => {
              const onSuccess = () => {
                setScheduled(null);
                setEditingId(null);
                setNotice(t("Scheduled message saved."));
              };
              if (editingId === null) addScheduled.mutate(scheduled, { onSuccess });
              else saveScheduled.mutate({ id: editingId, draft: scheduled }, { onSuccess });
            }}
          />
        )}
        {previewItem && (
          <div className="utility-overlay" role="presentation" onClick={() => setPreviewItem(null)}>
            {/* eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-noninteractive-element-interactions -- Dialog containment. */}
            <section
              className="utility-dialog"
              role="dialog"
              aria-modal="true"
              aria-label={t("Message preview")}
              onClick={(e) => e.stopPropagation()}
            >
              <div className="utility-dialog-head">
                <strong>{t("PREVIEW")}</strong>
                <button onClick={() => setPreviewItem(null)} aria-label={t("Close preview")}>
                  ×
                </button>
              </div>
              <MessagePreview
                preview={{ text: previewItem.content, embed: previewItem.embed, ephemeral: false, notes: [] }}
              />
              {runs.length > 0 && (
                <div className="utility-runs">
                  <strong>{t("NEXT RUNS")}</strong>
                  <ol>
                    {runs.map((run) => (
                      <li key={run}>{dateTime(run)}</li>
                    ))}
                  </ol>
                </div>
              )}
            </section>
          </div>
        )}
        {notice && (
          <div className="notice" role="status">
            {notice}
            <button onClick={() => setNotice(null)} aria-label={t("Dismiss")}>
              ×
            </button>
          </div>
        )}
        {showError(removeSticky.error || removeScheduled.error || enableScheduled.error)}
      </SettingsPage>
    </UtilityWords.Provider>
  );
}
