import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useOutletContext } from "react-router";
import { translate as t, dateTime } from "../../../i18n";
import { SettingsPage } from "../../shell/pages/PageHead";
import { Loading, Problem } from "../../shell/pages/States";
import { Panel } from "../../../components/Panel";
import { ChannelPicker } from "../../../components/Pickers";
import { type Overview, api } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import "../../../styles/community-tools.css";

interface Giveaway {
  id: string;
  prize: string;
  status: string;
  ends_at: string;
  starts_at: string | null;
  winners: number;
  entries: number;
}
interface Poll {
  id: string;
  question: string;
  status: string;
  options: string[];
  multiple: boolean;
  anonymous: boolean;
  ends_at: string;
}
interface Suggestion {
  id: string;
  number: number;
  body: string;
  status: string;
  staff_note: string | null;
  thread_id: string | null;
  up: number;
  down: number;
}
interface AutoChannel {
  channel_id: string;
  thread: boolean;
  crosspost: boolean;
}
interface Confession {
  id: string;
  body: string;
  status: string;
  created_at: string;
  reports: number;
}
interface Data {
  giveaways: Giveaway[];
  polls: Poll[];
  suggestions: Suggestion[];
  autopost: AutoChannel[];
  anniversary: { channel_id: string | null; enabled: boolean };
  confession_settings: {
    review_channel_id: string | null;
    publish_channel_id: string | null;
    enabled: boolean;
  };
  confessions: Confession[];
  message_content: boolean;
}
interface Personal {
  reminders: { id: string; body: string; interval_minutes: number; next_at: string }[];
  checklist: { id: string; body: string; done: boolean }[];
  highlights: string[];
}
function useTools() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useQuery({
    queryKey: ["community-v2", guild],
    queryFn: () => api<Data>(`/api/guilds/${guild}/community-v2`),
  });
  return { guild, query, discord: useDiscord(guild), client: useQueryClient() };
}
function useWrite(guild: string, client: ReturnType<typeof useQueryClient>) {
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  async function write(path: string, method: "PUT" | "DELETE", payload?: unknown) {
    setBusy(true);
    setNotice("");
    try {
      await api(`/api/guilds/${guild}/community-v2${path}`, method, payload);
      await client.invalidateQueries({ queryKey: ["community-v2", guild] });
      setNotice(t("Saved."));
      return true;
    } catch (error) {
      setNotice(error instanceof Error ? t(error.message) : t("Could not save."));
      return false;
    } finally {
      setBusy(false);
    }
  }
  return { busy, notice, write };
}
function State({ query }: { query: ReturnType<typeof useTools>["query"] }) {
  if (query.isPending) return <Loading label={t("Loading community tools")} />;
  if (query.isError) return <Problem error={query.error} />;
  return null;
}

export function CommunityGiveaways() {
  const { query } = useTools();
  return (
    <SettingsPage
      title={t("GIVEAWAYS")}
      lead={t("Schedule draws, set entry requirements and award bonus entries by role with /giveaway start.")}
    >
      <State query={query} />
      {query.data && (
        <Panel title={t("RECENT GIVEAWAYS")}>
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
        </Panel>
      )}
    </SettingsPage>
  );
}
export function CommunityPolls() {
  const { query } = useTools();
  return (
    <SettingsPage
      title={t("POLLS")}
      lead={t("Timed polls can allow multiple choices, anonymous authors and role restrictions with /poll.")}
    >
      <State query={query} />
      {query.data && (
        <Panel title={t("RECENT POLLS")}>
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
                    {p.multiple ? ` · ${t("Multiple choice")}` : ""}
                    {p.anonymous ? ` · ${t("Anonymous")}` : ""}
                  </span>
                </article>
              ))}
            </div>
          )}
        </Panel>
      )}
    </SettingsPage>
  );
}
export function CommunitySuggestions() {
  const { guild, query, client } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  return (
    <SettingsPage
      title={t("SUGGESTIONS")}
      lead={t("Review ideas, keep private staff notes and track their status. Members vote with /suggest.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <State query={query} />
      {query.data && (
        <Panel title={t("RECENT SUGGESTIONS")}>
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
        </Panel>
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
        <>
          <Panel title={t("RECURRING REMINDERS")}>
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
          </Panel>
          <Panel title={t("CHECKLIST")}>
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
          </Panel>
        </>
      )}
    </SettingsPage>
  );
}
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
      title={t("KEYWORD HIGHLIGHTS")}
      lead={t("Members opt in with /highlight add. A matching message sends a private link, never its text.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <State query={query} />
      {query.data && (
        <Panel title={t("AVAILABILITY")}>
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
        </Panel>
      )}
      {personal.isPending && <Loading label={t("Loading followed words")} />}
      {personal.isError && <Problem error={personal.error} />}
      {personal.data && (
        <Panel title={t("WORDS YOU FOLLOW")}>
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
              onClick={() => void write("/highlights", "PUT", { word }).then((saved) => saved && setWord(""))}
            >
              {t("Follow word")}
            </button>
          </div>
        </Panel>
      )}
    </SettingsPage>
  );
}
export function CommunityAutoChannels() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  const [channel, setChannel] = useState<string | null>(null);
  const [thread, setThread] = useState(false);
  const [crosspost, setCrosspost] = useState(false);
  return (
    <SettingsPage
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
        <>
          <Panel title={t("CHANNEL RULES")}>
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
          </Panel>
          <Panel title={t("ADD CHANNEL")}>
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
          </Panel>
        </>
      )}
    </SettingsPage>
  );
}
export function CommunityAnniversaries() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  if (!query.data)
    return (
      <SettingsPage
        title={t("MEMBER ANNIVERSARIES")}
        lead={t("Celebrate when a member reaches another year in your server.")}
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
      title={t("MEMBER ANNIVERSARIES")}
      lead={t("Celebrate when a member reaches another year in your server.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <Panel title={t("ANNOUNCEMENTS")}>
        <div className="community-v2-controls">
          <label>
            {t("Channel")}
            <ChannelPicker info={discord} value={channel} onChange={setChannel} />
          </label>
          <label>
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />{" "}
            {t("Enabled")}
          </label>
          <button
            className="pixel-button"
            disabled={!changed || busy || (enabled && !channel)}
            onClick={() => write("/anniversary", "PUT", { channel_id: channel, enabled })}
          >
            {t("Save")}
          </button>
        </div>
      </Panel>
    </SettingsPage>
  );
}
export function CommunityConfessions() {
  const { guild, query, client, discord } = useTools();
  const { busy, notice, write } = useWrite(guild, client);
  if (!query.data)
    return (
      <SettingsPage
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
      title={t("ANONYMOUS CONFESSIONS")}
      lead={t("Members submit anonymously; staff review before public posting.")}
    >
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
      <Panel title={t("REVIEW SETTINGS")}>
        <div className="community-v2-controls">
          <label>
            {t("Staff review channel")}
            <ChannelPicker info={discord} value={review} onChange={setReview} />
          </label>
          <label>
            {t("Public channel")}
            <ChannelPicker info={discord} value={publish} onChange={setPublish} />
          </label>
          <label>
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />{" "}
            {t("Enabled")}
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
      </Panel>
      <Panel title={t("REVIEW QUEUE")}>
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
      </Panel>
      <Panel title={t("REPORTED CONFESSIONS")}>
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
      </Panel>
    </SettingsPage>
  );
}
