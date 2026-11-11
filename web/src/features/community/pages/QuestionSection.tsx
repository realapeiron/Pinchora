import { locale, translate as t } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ChannelPicker } from "../../../components/Pickers";
import { useEngagementContext } from "./engagementContext";
import { questionText } from "../api/communityEngagement";

export function QuestionSection() {
  const { data, info, qotd, setQotd, busy, write, summary } = useEngagementContext();
  return (
    <SettingsRow
      title={t("QUESTION OF THE DAY")}
      description={dt("Post a daily question and manage its queue.")}
      summary={summary.qotd ?? dt("Off")}
      initialOpen
    >
      <p className="panel-lead muted">
        {t(
          "A question posts once a day in the server time zone. Members can suggest questions with /qotd suggest.",
        )}
      </p>
      <label>
        {t("Post channel")}
        <ChannelPicker
          info={info}
          value={qotd.channel_id}
          onChange={(value) => setQotd({ ...qotd, channel_id: value })}
          none={t("Choose a channel")}
        />
      </label>
      <div className="community-pack-inline">
        <label>
          {t("Hour (0–23)")}
          <input
            className="pixel-input"
            type="number"
            min="0"
            max="23"
            value={qotd.hour}
            onChange={(e) => setQotd({ ...qotd, hour: Number(e.target.value) })}
          />
        </label>
        <label>
          {t("Time zone")}
          <input
            className="pixel-input"
            value={qotd.timezone}
            onChange={(e) => setQotd({ ...qotd, timezone: e.target.value })}
          />
        </label>
      </div>
      <label>
        {t("Ping role")}
        <select
          className="pixel-input pixel-select"
          value={qotd.ping_role_id ?? ""}
          onChange={(e) => setQotd({ ...qotd, ping_role_id: e.target.value || null })}
        >
          <option value="">{t("No ping")}</option>
          {info?.roles.map((r) => (
            <option key={r.id} value={r.id}>
              {r.name}
            </option>
          ))}
        </select>
      </label>
      <label className="community-pack-check">
        <input
          type="checkbox"
          checked={qotd.thread}
          onChange={(e) => setQotd({ ...qotd, thread: e.target.checked })}
        />
        {t("Open a discussion thread")}
      </label>
      <button className="pixel-button" disabled={busy} onClick={() => void write("/qotd", "PUT", qotd)}>
        {t("Save QOTD")}
      </button>
      <h3>
        {t("Question queue")} <small>{data.questions.length}/500</small>
      </h3>
      <div className="community-pack-list">
        {data.questions.slice(0, 30).map((question) => (
          <article key={question.id}>
            <span className="muted">
              #{question.id} · {question.approved ? t("Approved") : t("Pending")}
            </span>
            <p>{questionText(question, locale())}</p>
            {!question.approved && (
              <div className="community-pack-actions">
                <button
                  disabled={busy}
                  onClick={() => void write(`/qotd/${question.id}/decision`, "PUT", { approve: true })}
                >
                  {t("Approve")}
                </button>
                <button
                  disabled={busy}
                  onClick={() => void write(`/qotd/${question.id}/decision`, "PUT", { approve: false })}
                >
                  {t("Deny")}
                </button>
              </div>
            )}
          </article>
        ))}
      </div>
    </SettingsRow>
  );
}
