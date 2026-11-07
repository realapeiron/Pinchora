import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useOutletContext } from "react-router";
import { locale, translate as t } from "../i18n";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import { type Overview, api } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type ApplicationForm,
  type Counting,
  type MailSettings,
  type QotdSettings,
  useCommunityPack,
  questionText,
} from "../lib/community-pack";
import "../styles/community-pack.css";

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
const emptyForm: ApplicationForm = {
  id: "",
  name: "",
  questions: [""],
  submit_channel_id: null,
  review_roles: [],
  accept_role_id: null,
  dm_result: true,
  open: true,
  message_id: null,
};

export function CommunityPack() {
  const overview = useOutletContext<Overview>();
  const guild = overview.guild.id;
  const query = useCommunityPack(guild);
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
    if (!query.data) return;
    setQotd(query.data.qotd ?? emptyQotd);
    setCounting(query.data.counting ?? emptyCounting);
    setMail(query.data.modmail ?? emptyMail);
  }, [query.data]);
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
  return (
    <div className="settings-page community-pack-page">
      <PageHead
        title={t("COMMUNITY PACK")}
        lead={t(
          "Give members a daily question, a counting channel, applications and a private path to staff.",
        )}
      />
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
          <div className="community-pack-grid">
            <Panel title={t("QUESTION OF THE DAY")} tone="accent">
              <p className="panel-lead muted">
                {t(
                  "A question posts once a day in the server time zone. Members can suggest questions with /qotd suggest.",
                )}
              </p>
              <label>
                {t("Post channel")}
                <ChannelPicker
                  info={discord.data}
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
                  {discord.data?.roles.map((r) => (
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
              <button
                className="pixel-button"
                disabled={busy}
                onClick={() => void write("/qotd", "PUT", qotd)}
              >
                {t("Save QOTD")}
              </button>
              <h3>
                {t("Question queue")} <small>{query.data.questions.length}/500</small>
              </h3>
              <div className="community-pack-list">
                {query.data.questions.slice(0, 30).map((question) => (
                  <article key={question.id}>
                    <span className="muted">
                      #{question.id} · {question.approved ? t("Approved") : t("Pending")}
                    </span>
                    <p>{questionText(question, locale())}</p>
                    {!question.approved && (
                      <div className="community-pack-actions">
                        <button
                          disabled={busy}
                          onClick={() =>
                            void write(`/qotd/${question.id}/decision`, "PUT", { approve: true })
                          }
                        >
                          {t("Approve")}
                        </button>
                        <button
                          disabled={busy}
                          onClick={() =>
                            void write(`/qotd/${question.id}/decision`, "PUT", { approve: false })
                          }
                        >
                          {t("Deny")}
                        </button>
                      </div>
                    )}
                  </article>
                ))}
              </div>
            </Panel>
            <Panel title={t("COUNTING")} tone="gold">
              <p className="panel-lead muted">
                {t(
                  "One member posts the next number. A second turn by the same member fails. Simple arithmetic is optional.",
                )}
              </p>
              <p className="community-pack-metric">
                {counting.current}
                <small>{t("Current count")}</small>
              </p>
              <p className="muted">
                {t("Record")}: {counting.record}
              </p>
              <label>
                {t("Counting channel")}
                <ChannelPicker
                  info={discord.data}
                  value={counting.channel_id || null}
                  onChange={(value) => setCounting({ ...counting, channel_id: value ?? "" })}
                  none={t("Choose a channel")}
                />
              </label>
              <label className="community-pack-check">
                <input
                  type="checkbox"
                  checked={counting.reset_on_fail}
                  onChange={(e) => setCounting({ ...counting, reset_on_fail: e.target.checked })}
                />
                {t("Reset on a mistake")}
              </label>
              <label className="community-pack-check">
                <input
                  type="checkbox"
                  checked={counting.allow_math}
                  onChange={(e) => setCounting({ ...counting, allow_math: e.target.checked })}
                />
                {t("Allow small arithmetic")}
              </label>
              <label>
                {t("Failure role")}
                <select
                  className="pixel-input pixel-select"
                  value={counting.fail_role_id ?? ""}
                  onChange={(e) => setCounting({ ...counting, fail_role_id: e.target.value || null })}
                >
                  <option value="">{t("No role")}</option>
                  {discord.data?.roles
                    .filter((r) => r.action_ok && r.assignable)
                    .map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name}
                      </option>
                    ))}
                </select>
              </label>
              <div className="community-pack-actions">
                <button
                  className="pixel-button"
                  disabled={busy || !counting.channel_id}
                  onClick={() => void write("/counting", "PUT", counting)}
                >
                  {t("Save counting")}
                </button>
                <button
                  disabled={busy || !query.data.counting}
                  onClick={() => void write("/counting/reset", "POST")}
                >
                  {t("Reset count")}
                </button>
              </div>
            </Panel>
            <Panel title={t("APPLICATION FORMS")}>
              <p className="panel-lead muted">
                {t(
                  "Build up to ten forms with five questions each. Applicants answer in a private Discord modal; staff review submissions here.",
                )}
              </p>
              <button
                className="pixel-button"
                disabled={query.data.forms.length >= 10}
                onClick={() => setForm({ ...emptyForm, questions: [""] })}
              >
                {t("New form")}
              </button>
              {form && (
                <div className="community-pack-editor">
                  <label>
                    {t("Form name")}
                    <input
                      className="pixel-input"
                      maxLength={45}
                      value={form.name}
                      onChange={(e) => setForm({ ...form, name: e.target.value })}
                    />
                  </label>
                  {form.questions.map((question, index) => (
                    <label key={index}>
                      {t("Question {number}", { number: index + 1 })}
                      <input
                        className="pixel-input"
                        maxLength={45}
                        value={question}
                        onChange={(e) =>
                          setForm({
                            ...form,
                            questions: form.questions.map((q, i) => (i === index ? e.target.value : q)),
                          })
                        }
                      />
                    </label>
                  ))}
                  <button
                    disabled={form.questions.length >= 5}
                    onClick={() => setForm({ ...form, questions: [...form.questions, ""] })}
                  >
                    {t("Add question")}
                  </button>
                  <label>
                    {t("Review channel")}
                    <ChannelPicker
                      info={discord.data}
                      value={form.submit_channel_id}
                      onChange={(value) => setForm({ ...form, submit_channel_id: value })}
                      none={t("Choose a channel")}
                    />
                  </label>
                  <label>
                    {t("Acceptance role")}
                    <select
                      className="pixel-input pixel-select"
                      value={form.accept_role_id ?? ""}
                      onChange={(e) => setForm({ ...form, accept_role_id: e.target.value || null })}
                    >
                      <option value="">{t("No role")}</option>
                      {discord.data?.roles
                        .filter((r) => r.action_ok && r.assignable)
                        .map((r) => (
                          <option key={r.id} value={r.id}>
                            {r.name}
                          </option>
                        ))}
                    </select>
                  </label>
                  <label className="community-pack-check">
                    <input
                      type="checkbox"
                      checked={form.dm_result}
                      onChange={(e) => setForm({ ...form, dm_result: e.target.checked })}
                    />
                    {t("DM the decision")}
                  </label>
                  <fieldset className="community-pack-roles">
                    <legend>{t("Review roles")}</legend>
                    {discord.data?.roles.map((role) => (
                      <label key={role.id} className="community-pack-check">
                        <input
                          type="checkbox"
                          checked={form.review_roles.includes(role.id)}
                          onChange={(e) =>
                            setForm({
                              ...form,
                              review_roles: e.target.checked
                                ? [...form.review_roles, role.id]
                                : form.review_roles.filter((id) => id !== role.id),
                            })
                          }
                        />
                        {role.name}
                      </label>
                    ))}
                  </fieldset>
                  <label className="community-pack-check">
                    <input
                      type="checkbox"
                      checked={form.open}
                      onChange={(e) => setForm({ ...form, open: e.target.checked })}
                    />
                    {t("Accept applications")}
                  </label>
                  <div className="community-pack-actions">
                    <button
                      className="pixel-button"
                      disabled={busy || !form.submit_channel_id}
                      onClick={async () => {
                        const path = form.id ? `/forms/${form.id}` : "/forms";
                        if (
                          await write(path, form.id ? "PUT" : "POST", {
                            ...form,
                            questions: form.questions.filter(Boolean),
                            submit_channel_id: form.submit_channel_id,
                          })
                        )
                          setForm(null);
                      }}
                    >
                      {t("Save form")}
                    </button>
                    <button onClick={() => setForm(null)}>{t("Cancel")}</button>
                  </div>
                </div>
              )}
              <div className="community-pack-list">
                <label>
                  {t("Post form in")}
                  <ChannelPicker
                    info={discord.data}
                    value={postChannel}
                    onChange={setPostChannel}
                    none={t("Choose a channel")}
                  />
                </label>
                {query.data.forms.map((item) => (
                  <article key={item.id}>
                    <strong>{item.name}</strong>
                    <span className="muted">
                      {" "}
                      #{item.id} · {item.open ? t("Open") : t("Closed")}
                    </span>
                    <p className="muted">
                      {item.questions.length} {t("questions")}
                    </p>
                    <button onClick={() => setForm({ ...item })}>{t("Edit")}</button>{" "}
                    <button
                      disabled={busy || !postChannel || !item.open}
                      onClick={() =>
                        void write(`/forms/${item.id}/post`, "POST", { channel_id: postChannel })
                      }
                    >
                      {t("Post button")}
                    </button>
                  </article>
                ))}
              </div>
              <h3>{t("Recent submissions")}</h3>
              <div className="community-pack-list">
                {query.data.submissions.map((item) => (
                  <article key={item.id}>
                    <strong>#{item.id}</strong> ·{" "}
                    <span className="muted">
                      {item.user_id} ·{" "}
                      {item.status === "pending"
                        ? t("Pending")
                        : item.status === "accepted"
                          ? t("Approved")
                          : t("Declined")}
                    </span>
                    <p>
                      {item.answers.map((answer, i) => (
                        <span key={i} className="community-pack-answer">
                          {i + 1}. {answer}
                        </span>
                      ))}
                    </p>
                    {item.status === "pending" && (
                      <div className="community-pack-actions">
                        <button
                          disabled={busy}
                          onClick={() =>
                            void write(`/forms/${item.form_id}/submissions/${item.id}`, "PUT", {
                              accept: true,
                            })
                          }
                        >
                          {t("Accept")}
                        </button>
                        <button
                          disabled={busy}
                          onClick={() =>
                            void write(`/forms/${item.form_id}/submissions/${item.id}`, "PUT", {
                              accept: false,
                            })
                          }
                        >
                          {t("Deny")}
                        </button>
                      </div>
                    )}
                  </article>
                ))}
              </div>
            </Panel>
            <Panel title={t("MODMAIL")}>
              <p className="panel-lead muted">
                {t(
                  "Members DM Pinchora; staff answer in a private forum thread. Use /modmail for snippets, blocks and closing from Discord.",
                )}
              </p>
              <label className="community-pack-check">
                <input
                  type="checkbox"
                  checked={mail.enabled}
                  onChange={(e) => setMail({ ...mail, enabled: e.target.checked })}
                />
                {t("Enable modmail")}
              </label>
              <label>
                {t("Staff forum")}
                <select
                  className="pixel-input pixel-select"
                  value={mail.forum_channel_id ?? ""}
                  onChange={(e) => setMail({ ...mail, forum_channel_id: e.target.value || null })}
                >
                  <option value="">{t("Choose a forum")}</option>
                  {discord.data?.channels
                    .filter((c) => c.type === 15)
                    .map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                </select>
              </label>
              <label>
                {t("Greeting")}
                <textarea
                  className="pixel-input"
                  maxLength={500}
                  value={mail.greeting}
                  onChange={(e) => setMail({ ...mail, greeting: e.target.value })}
                />
              </label>
              <label>
                {t("Close after hours")}
                <input
                  className="pixel-input"
                  type="number"
                  min="1"
                  max="8760"
                  value={mail.close_after_hours}
                  onChange={(e) => setMail({ ...mail, close_after_hours: Number(e.target.value) })}
                />
              </label>
              <label className="community-pack-check">
                <input
                  type="checkbox"
                  checked={mail.anonymous_staff}
                  onChange={(e) => setMail({ ...mail, anonymous_staff: e.target.checked })}
                />
                {t("Hide staff names")}
              </label>
              <fieldset className="community-pack-roles">
                <legend>{t("Staff roles")}</legend>
                {discord.data?.roles.map((role) => (
                  <label key={role.id} className="community-pack-check">
                    <input
                      type="checkbox"
                      checked={mail.staff_roles.includes(role.id)}
                      onChange={(e) =>
                        setMail({
                          ...mail,
                          staff_roles: e.target.checked
                            ? [...mail.staff_roles, role.id]
                            : mail.staff_roles.filter((id) => id !== role.id),
                        })
                      }
                    />
                    {role.name}
                  </label>
                ))}
              </fieldset>
              <button
                className="pixel-button"
                disabled={busy}
                onClick={() => void write("/modmail", "PUT", mail)}
              >
                {t("Save modmail")}
              </button>
              <h3>{t("Open threads")}</h3>
              <div className="community-pack-list">
                {query.data.threads.map((thread) => (
                  <article key={thread.id}>
                    <strong>#{thread.id}</strong> · <span className="muted">{thread.user_id}</span>
                    <div className="community-pack-actions">
                      <a
                        href={`https://discord.com/channels/${guild}/${thread.thread_id}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        {t("Open in Discord")}
                      </a>
                      <button
                        disabled={busy}
                        onClick={() => void write(`/modmail/${thread.id}/close`, "POST")}
                      >
                        {t("Close")}
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            </Panel>
          </div>
          <p className="muted">
            {t("Members use")} <code>/qotd suggest</code>, <code>/counting stats</code> {t("and")}{" "}
            <code>/form list</code>. <Link to={`/g/${guild}/modules`}>{t("Modules")}</Link>{" "}
            {t("can turn each feature off.")}
          </p>
        </>
      )}
    </div>
  );
}
