import { t as deferredText } from "../../../i18n/dashboard";
import { translate as t } from "../../../i18n";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { useEngagementContext } from "./engagementContext";

export function ModmailSection() {
  const { data, info, guild, mail, setMail, busy, write, summary } = useEngagementContext();
  return (
    <SettingsRow
      rowId="modmail-section-modmail"
      title={dt("Modmail", {}, "MODMAIL")}
      description={dt("Route member messages to staff threads.")}
      summary={summary.modmail ?? dt("Off")}
      enabled={mail.enabled}
      onToggle={(enabled) => setMail({ ...mail, enabled })}
      toggleLabel={deferredText("Enable modmail")}
    >
      <p className="panel-lead muted">
        {deferredText(
          "Members DM Pinchora; staff answer in a private forum thread. Use /modmail for snippets, blocks and closing from Discord.",
        )}
      </p>
      <label>
        {deferredText("Staff forum")}
        <select
          className="pixel-input pixel-select"
          value={mail.forum_channel_id ?? ""}
          onChange={(e) => setMail({ ...mail, forum_channel_id: e.target.value || null })}
        >
          <option value="">{deferredText("Choose a forum")}</option>
          {info?.channels
            .filter((c) => c.type === 15)
            .map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
        </select>
      </label>
      <label>
        {deferredText("Greeting")}
        <textarea
          className="pixel-input"
          maxLength={500}
          value={mail.greeting}
          onChange={(e) => setMail({ ...mail, greeting: e.target.value })}
        />
      </label>
      <label>
        {deferredText("Close after hours")}
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
        {deferredText("Hide staff names")}
      </label>
      <fieldset className="community-pack-roles">
        <legend>{deferredText("Staff roles")}</legend>
        {info?.roles.map((role) => (
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
      <button className="pixel-button" disabled={busy} onClick={() => void write("/modmail", "PUT", mail)}>
        {deferredText("Save modmail")}
      </button>
      <h3>{deferredText("Open threads")}</h3>
      <div className="community-pack-list">
        {data.threads.map((thread) => (
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
              <button disabled={busy} onClick={() => void write(`/modmail/${thread.id}/close`, "POST")}>
                {t("Close")}
              </button>
            </div>
          </article>
        ))}
      </div>
    </SettingsRow>
  );
}
