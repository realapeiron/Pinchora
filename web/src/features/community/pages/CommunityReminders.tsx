import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { translate as t, dateTime } from "../../../i18n";
import { SettingsPage } from "../../../components/PageHead";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { api } from "../../../lib/api";
import { Loading, Problem } from "../../../components/States";
import { useTools, type Personal } from "./communityToolsShared";
import { personalSummary } from "./communitySummary";

export function CommunityReminders() {
  const { guild, client } = useTools();
  const personal = useQuery({
    queryKey: ["community-v2-personal", guild],
    queryFn: () => api<Personal>(`/api/guilds/${guild}/community-v2/personal`),
  });
  const [text, setText] = useState("");
  const [hours, setHours] = useState(24);
  const [task, setTask] = useState("");
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
      title={t("REMINDERS & CHECKLISTS")}
      lead={t("Personal tools are private to you and sync with /remind and /checklist in Discord.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      {personal.isPending && <Loading label={t("Loading personal tools")} />}
      {personal.isError && <Problem error={personal.error} />}
      {personal.data && (
        <div className="settings-detail-list">
          <SettingsRow
            title={t("RECURRING REMINDERS")}
            description={dt("Schedule and remove private recurring reminders.")}
            summary={personalSummary(personal.data).reminders}
            initialOpen
          >
            <div className="community-v2-list">
              {personal.data.reminders.map((r) => (
                <article key={r.id} className="community-v2-item">
                  <strong>{r.body}</strong>
                  <span className="muted">
                    {t("Every {hours} hours", { hours: r.interval_minutes / 60 })} · {dateTime(r.next_at)}
                  </span>
                  <button
                    className="pixel-button"
                    disabled={busy}
                    onClick={() => void write(`/reminders/${r.id}`, "DELETE")}
                  >
                    {t("Remove")}
                  </button>
                </article>
              ))}
            </div>
            <div className="community-v2-controls">
              <label>
                {t("Reminder text")}
                <input
                  className="pixel-input"
                  maxLength={200}
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                />
              </label>
              <label>
                {t("Repeat every (hours)")}
                <input
                  className="pixel-input"
                  type="number"
                  min={1}
                  max={8760}
                  value={hours}
                  onChange={(e) => setHours(Number(e.target.value))}
                />
              </label>
              <button
                className="pixel-button"
                disabled={
                  !text.trim() ||
                  !Number.isInteger(hours * 60) ||
                  hours < 1 ||
                  hours > 8760 ||
                  busy ||
                  personal.data.reminders.length >= 25
                }
                onClick={() =>
                  void write("/reminders", "PUT", { text, interval_minutes: hours * 60 }).then(
                    (saved) => saved && setText(""),
                  )
                }
              >
                {t("Add reminder")}
              </button>
            </div>
          </SettingsRow>
          <SettingsRow
            title={t("CHECKLIST")}
            description={dt("Track your private tasks.")}
            summary={personalSummary(personal.data).checklist}
          >
            <div className="community-v2-list">
              {personal.data.checklist.map((c) => (
                <article className="community-v2-item" key={c.id}>
                  <strong>
                    {c.done ? "☑" : "☐"} {c.body}
                  </strong>
                  <div className="community-v2-actions">
                    {!c.done && (
                      <button
                        className="pixel-button"
                        disabled={busy}
                        onClick={() => void write(`/checklist/${c.id}`, "PUT")}
                      >
                        {t("Done")}
                      </button>
                    )}
                    <button
                      className="pixel-button"
                      disabled={busy}
                      onClick={() => void write(`/checklist/${c.id}`, "DELETE")}
                    >
                      {t("Remove")}
                    </button>
                  </div>
                </article>
              ))}
            </div>
            <div className="community-v2-controls">
              <label>
                {t("New task")}
                <input
                  className="pixel-input"
                  maxLength={200}
                  value={task}
                  onChange={(e) => setTask(e.target.value)}
                />
              </label>
              <button
                className="pixel-button"
                disabled={!task.trim() || busy || personal.data.checklist.filter((c) => !c.done).length >= 50}
                onClick={() =>
                  void write("/checklist", "PUT", { text: task }).then((saved) => saved && setTask(""))
                }
              >
                {t("Add task")}
              </button>
            </div>
          </SettingsRow>
        </div>
      )}
    </SettingsPage>
  );
}
