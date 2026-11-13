import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { translate as t, dateTime } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { State, useTools, useWrite, type Suggestion } from "./communityToolsShared";
import { communitySummary } from "./communitySummary";

export function CommunityGiveaways() {
  const { query } = useTools();
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("GIVEAWAYS")}
      lead={deferredText(
        "Schedule draws, set entry requirements and award bonus entries by role with /giveaway start.",
      )}
    >
      <State query={query} />
      {query.data && (
        <SettingsRow
          rowId="community-lists-recent-giveaways"
          title={dt("Recent giveaways", {}, "RECENT GIVEAWAYS")}
          description={dt("See recent draws and their entry totals.")}
          summary={communitySummary(query.data).giveaways}
          initialOpen
        >
          {query.data.giveaways.length === 0 ? (
            <p className="muted">{t("No giveaways yet.")}</p>
          ) : (
            <div className="community-v2-list">
              {query.data.giveaways.map((g) => (
                <article key={g.id} className="community-v2-item">
                  <strong>
                    #{g.id} · {g.prize}
                  </strong>
                  <span className="muted">
                    {t(g.status)} · {g.entries} {t("entries")} · {g.winners} {t("winners")} ·{" "}
                    {dateTime(g.ends_at)}
                  </span>
                </article>
              ))}
            </div>
          )}
        </SettingsRow>
      )}
    </SettingsPage>
  );
}
export function CommunityPolls() {
  const { query } = useTools();
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("POLLS")}
      lead={deferredText(
        "Timed polls can allow multiple choices, anonymous authors and role restrictions with /poll.",
      )}
    >
      <State query={query} />
      {query.data && (
        <SettingsRow
          rowId="community-lists-recent-polls"
          title={dt("Recent polls", {}, "RECENT POLLS")}
          description={dt("See saved polls and their choices.")}
          summary={communitySummary(query.data).polls}
        >
          {query.data.polls.length === 0 ? (
            <p className="muted">{t("No polls yet.")}</p>
          ) : (
            <div className="community-v2-list">
              {query.data.polls.map((p) => (
                <article key={p.id} className="community-v2-item">
                  <strong>
                    #{p.id} · {p.question}
                  </strong>
                  <span className="muted">
                    {t(p.status)} · {p.options.length} {t("choices")} · {dateTime(p.ends_at)}
                    {p.multiple ? ` · ${deferredText("Multiple choice")}` : ""}
                    {p.anonymous ? ` · ${deferredText("Anonymous")}` : ""}
                  </span>
                </article>
              ))}
            </div>
          )}
        </SettingsRow>
      )}
    </SettingsPage>
  );
}
export function CommunitySuggestions() {
  const { guild, query, client } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  return (
    <SettingsPage
      className="settings-page community-tools-page"
      title={t("SUGGESTIONS")}
      lead={deferredText(
        "Review ideas, keep private staff notes and track their status. Members vote with /suggest.",
      )}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <State query={query} />
      {query.data && (
        <SettingsRow
          rowId="community-lists-recent-suggestions"
          title={dt("Recent suggestions", {}, "RECENT SUGGESTIONS")}
          description={dt("Review member ideas and staff notes.")}
          summary={communitySummary(query.data).suggestions}
        >
          {query.data.suggestions.length === 0 ? (
            <p className="muted">{t("No suggestions yet.")}</p>
          ) : (
            <div className="community-v2-list">
              {query.data.suggestions.map((s) => (
                <SuggestionEditor
                  key={s.id}
                  value={s}
                  busy={busy}
                  save={(status, staff_note) => write(`/suggestions/${s.id}`, "PUT", { status, staff_note })}
                />
              ))}
            </div>
          )}
        </SettingsRow>
      )}
    </SettingsPage>
  );
}
function SuggestionEditor({
  value,
  busy,
  save,
}: {
  value: Suggestion;
  busy: boolean;
  save: (status: string, staff_note: string | null) => void;
}) {
  const [status, setStatus] = useState(value.status);
  const [note, setNote] = useState(value.staff_note ?? "");
  const changed = status !== value.status || note !== (value.staff_note ?? "");
  return (
    <article className="community-v2-item">
      <strong>
        #{value.number} · {value.body}
      </strong>
      <span className="muted">
        👍 {value.up} · 👎 {value.down}
        {value.thread_id ? ` · ${t("Thread")}: ${value.thread_id}` : ""}
      </span>
      <div className="community-v2-controls">
        <label>
          {t("Status")}
          <select
            className="pixel-input pixel-select"
            value={status}
            onChange={(e) => setStatus(e.target.value)}
          >
            {["open", "considering", "approved", "implemented", "denied"].map((s) => (
              <option key={s} value={s}>
                {t(s)}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Staff note")}
          <textarea
            className="pixel-input"
            maxLength={1000}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </label>
        <button
          className="pixel-button"
          disabled={!changed || busy}
          onClick={() => save(status, note || null)}
        >
          {t("Save")}
        </button>
      </div>
    </article>
  );
}
