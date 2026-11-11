import { dateTime } from "../../../i18n";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { ChannelPicker } from "../../../components/Pickers";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./safetySummary";
import type { GuildInfo } from "../../../lib/api";
import {
  idList,
  type Gate,
  type Raid,
  type SafetyData,
  type useSafety,
  type words,
} from "../api/verification";

type Text = ReturnType<typeof words>;
type Actions = ReturnType<typeof useSafety>;
type Change = (section: "verification" | "raid", key: string, value: unknown) => void;
export function SafetyMessage({
  v,
  t,
  summary,
  dirty,
  post,
  preview,
  onChange,
}: {
  v: Gate;
  t: Text;
  summary: string;
  dirty: boolean;
  post: Actions["post"];
  preview: Actions["preview"];
  onChange: Change;
}) {
  return (
    <SettingsRow
      rowId="safety-rows-message"
      title={t("message")}
      description={dashboardText("Edit the verification message and captcha preview.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <Field
          label={t("message")}
          htmlFor="safety-message"
          aside={t("used", { count: v.message.length, max: 1500 })}
        >
          <textarea
            id="safety-message"
            className="pixel-input"
            rows={5}
            maxLength={1500}
            value={v.message}
            onChange={(e) => onChange("verification", "message", e.target.value)}
          />
        </Field>
        <PixelButton
          variant="secondary"
          disabled={post.isPending || dirty || !v.channel_id}
          onClick={() => post.mutate()}
        >
          {t("post")}
        </PixelButton>
      </div>
      <div className="settings-detail-preview">
        <div className="safety-preview">
          {preview.data ? (
            <img src={preview.data.image} width={640} height={280} alt={t("preview_alt")} />
          ) : (
            <p className="muted">{t("captcha")}</p>
          )}
        </div>
        <PixelButton variant="ghost" disabled={preview.isPending} onClick={() => preview.mutate()}>
          {t("preview")}
        </PixelButton>
      </div>
    </SettingsRow>
  );
}

export function SafetyExemptions({
  v,
  r,
  t,
  summary,
  discord,
  usersText,
  setUsersText,
  onChange,
}: {
  v: Gate;
  r: Raid;
  t: Text;
  summary: string;
  discord: GuildInfo | undefined;
  usersText: string | null;
  setUsersText: (value: string | null) => void;
  onChange: Change;
}) {
  return (
    <SettingsRow
      rowId="safety-rows-exempt-roles"
      title={t("exempt_roles")}
      description={t("roles_hint")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <p className="muted">{t("roles_hint")}</p>
        <Field
          label={t("exempt_roles")}
          htmlFor="safety-exempt-roles"
          aside={t("used", { count: v.exempt_roles.length, max: 50 })}
        >
          <select
            id="safety-exempt-roles"
            className="pixel-input safety-multi"
            multiple
            value={v.exempt_roles}
            onChange={(e) =>
              onChange(
                "verification",
                "exempt_roles",
                Array.from(e.target.selectedOptions, (o) => o.value),
              )
            }
          >
            {discord?.roles.map((role) => (
              <option key={role.id} value={role.id}>
                {role.name}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label={t("exempt_users")}
          htmlFor="safety-exempt-users"
          aside={t("used", { count: v.exempt_users.length, max: 50 })}
        >
          <textarea
            id="safety-exempt-users"
            className="pixel-input"
            value={usersText ?? v.exempt_users.join(", ")}
            onChange={(e) => {
              setUsersText(e.target.value);
              onChange("verification", "exempt_users", idList(e.target.value));
            }}
          />
        </Field>
        <Field label={t("alert_channel")} htmlFor="safety-alert">
          <ChannelPicker
            id="safety-alert"
            info={discord}
            value={r.alert_channel_id}
            none="—"
            onChange={(id) => onChange("raid", "alert_channel_id", id)}
          />
        </Field>
        <Field label={t("log_channel")} htmlFor="safety-log">
          <ChannelPicker
            id="safety-log"
            info={discord}
            value={v.log_channel_id}
            none="—"
            onChange={(id) => onChange("verification", "log_channel_id", id)}
          />
        </Field>
      </div>
    </SettingsRow>
  );
}

export function SafetyJoins({ saved, t, summary }: { saved: SafetyData; t: Text; summary: string }) {
  return (
    <SettingsRow
      rowId="safety-rows-joins"
      title={t("joins")}
      description={dashboardText("Review recent verification joins.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <div className="safety-table">
          <table>
            <thead>
              <tr>
                <th>{t("member")}</th>
                <th>{t("when")}</th>
                <th>{t("result")}</th>
              </tr>
            </thead>
            <tbody>
              {saved.joins.map((j) => (
                <tr key={j.user_id}>
                  <td>{j.user_id}</td>
                  <td>{dateTime(j.joined_at)}</td>
                  <td>{t(j.status)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {saved.joins.length === 0 && <p className="empty-row muted">{t("empty")}</p>}
        </div>
      </div>
    </SettingsRow>
  );
}

export function SafetyEvents({ saved, t, summary }: { saved: SafetyData; t: Text; summary: string }) {
  return (
    <SettingsRow
      rowId="safety-rows-events"
      title={t("events")}
      description={dashboardText("Review recent raid events.")}
      summary={summary}
    >
      <div className="settings-detail-fields">
        <div className="safety-events">
          {saved.events.map((e) => (
            <article key={e.id}>
              <strong>{t("raid_id", { id: e.id })}</strong>
              <span>{t(e.action)}</span>
              <time>{dateTime(e.started_at)}</time>
              {e.error && <p className="field-error">{e.error}</p>}
            </article>
          ))}
          {saved.events.length === 0 && <p className="empty-row muted">{t("empty")}</p>}
        </div>
      </div>
    </SettingsRow>
  );
}
