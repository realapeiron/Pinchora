import { translate as t } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ChannelPicker } from "../../../components/Pickers";
import { useEngagementContext } from "./engagementContext";
import { type ApplicationForm, applicationSettings } from "../api/communityEngagement";
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

export function ApplicationSection() {
  const { data, info, form, setForm, postChannel, setPostChannel, busy, write, summary } =
    useEngagementContext();
  return (
    <SettingsRow
      rowId="application-section-application-forms"
      title={dt("Application forms", {}, "APPLICATION FORMS")}
      description={dt("Build forms and review submissions.")}
      summary={summary.forms ?? dt("No application forms yet")}
    >
      <p className="panel-lead muted">
        {t(
          "Build up to ten forms with five questions each. Applicants answer in a private Discord modal; staff review submissions here.",
        )}
      </p>
      <button
        className="pixel-button"
        disabled={data.forms.length >= 10}
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
              info={info}
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
              {info?.roles
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
            {info?.roles.map((role) => (
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
                    ...applicationSettings(form),
                    questions: form.questions.filter(Boolean),
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
            info={info}
            value={postChannel}
            onChange={setPostChannel}
            none={t("Choose a channel")}
          />
        </label>
        {data.forms.map((item) => (
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
              onClick={() => void write(`/forms/${item.id}/post`, "POST", { channel_id: postChannel })}
            >
              {t("Post button")}
            </button>
          </article>
        ))}
      </div>
      <h3>{t("Recent submissions")}</h3>
      <div className="community-pack-list">
        {data.submissions.map((item) => (
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
    </SettingsRow>
  );
}
