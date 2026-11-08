import { dateTime } from "../i18n";
import { useState } from "react";
import { useOutletContext } from "react-router";
import type { Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import { idList, useSafety, words, type SafetyData, type SafetyConfig } from "../lib/verification";
import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { PixelToggle } from "../components/PixelToggle";
import { PixelButton } from "../components/PixelButton";
import { ChannelPicker, RolePicker } from "../components/Pickers";
import { SafetySaveBar } from "./SafetySaveBar";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/verification.css";
export function Safety() {
  const overview = useOutletContext<Overview>();
  const { query } = useSafety(overview.guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Editor guild={overview.guild.id} saved={query.data} />;
}
function Editor({ guild, saved }: { guild: string; saved: SafetyData }) {
  const t = words(saved.strings, saved.language);
  const [draft, setDraft] = useState<SafetyConfig | null>(null);
  const [usersText, setUsersText] = useState<string | null>(null);
  const c = draft ?? saved.config;
  const v = c.verification;
  const r = c.raid;
  const { save, raid, post, preview } = useSafety(guild);
  const discord = useDiscord(guild);
  const change = <K extends keyof SafetyConfig>(section: K, key: keyof SafetyConfig[K], value: unknown) =>
    setDraft({ ...c, [section]: { ...c[section], [key]: value } });
  const dirty = JSON.stringify(c) !== JSON.stringify(saved.config);
  const active = saved.events.find((e) => !e.ended_at);
  const toggle = (section: "verification" | "raid", key: string, label: string, value: boolean) => (
    <SwitchRow
      title={t(label)}
      control={
        <PixelToggle
          checked={value}
          label={t(label)}
          onChange={(next) => change(section, key as never, next)}
        />
      }
    />
  );
  const number = (
    section: "verification" | "raid",
    key: string,
    label: string,
    value: number,
    min: number,
    max: number,
  ) => (
    <Field label={t(label)} htmlFor={`safety-${key}`}>
      <input
        className="pixel-input"
        id={`safety-${key}`}
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(e) => change(section, key as never, Number(e.target.value))}
      />
    </Field>
  );
  const select = (
    section: "verification" | "raid",
    key: string,
    label: string,
    value: string,
    options: string[],
  ) => (
    <Field label={t(label)} htmlFor={`safety-${key}`}>
      <select
        className="pixel-input pixel-select"
        id={`safety-${key}`}
        value={value}
        onChange={(e) => change(section, key as never, e.target.value)}
      >
        {options.map((o) => (
          <option key={o} value={o}>
            {t(o)}
          </option>
        ))}
      </select>
    </Field>
  );
  const error = save.error ?? raid.error ?? post.error ?? preview.error;
  return (
    <SettingsPage className="settings-page safety-page" title={t("title")} lead={t("lead")}>
      <section className={`safety-status ${active ? "is-active" : ""}`} aria-label={t("status")}>
        <div>
          <h2 className="pixel-title">{t(active ? "active" : "calm")}</h2>
          {active && <p>{t("restores", { time: dateTime(active.expires_at) })}</p>}
        </div>
        <PixelButton
          variant={active ? "secondary" : "ghost"}
          disabled={raid.isPending}
          onClick={() => raid.mutate(active ? "unlock" : "lockdown")}
        >
          {t(active ? "end" : "start")}
        </PixelButton>
      </section>
      {active?.error && (
        <p className="field-error" role="alert">
          {active.error}
        </p>
      )}
      {error && (
        <p className="field-error" role="alert">
          {error.message}
        </p>
      )}
      <div className="safety-columns">
        <div className="safety-main">
          <Panel title={t("gate")}>
            {toggle("verification", "enabled", "enabled", v.enabled)}
            <p className="muted">{t("gate_hint")}</p>
            {select("verification", "mode", "mode", v.mode, ["button", "captcha", "web"])}
            <div className="safety-fields">
              {(["verified_role_id", "unverified_role_id"] as const).map((key) => (
                <Field
                  key={key}
                  label={t(key === "verified_role_id" ? "verified_role" : "unverified_role")}
                  htmlFor={`safety-${key}`}
                >
                  <RolePicker
                    id={`safety-${key}`}
                    info={discord.data}
                    value={v[key]}
                    none="—"
                    onChange={(id) => change("verification", key, id)}
                  />
                </Field>
              ))}
            </div>
            <p className="field-hint muted">{t("onboarding")}</p>
            <div className="safety-fields">
              <Field label={t("channel")} htmlFor="safety-channel">
                <ChannelPicker
                  id="safety-channel"
                  info={discord.data}
                  value={v.channel_id}
                  none="—"
                  onChange={(id) => change("verification", "channel_id", id)}
                />
              </Field>
              {number("verification", "kick_after_mins", "timeout", v.kick_after_mins, 0, 10080)}
              {number("verification", "min_account_age_hours", "age", v.min_account_age_hours, 0, 8760)}
              {select("verification", "rule_action", "rule_action", v.rule_action, [
                "hold",
                "notify",
                "kick",
              ])}
            </div>
            {toggle("verification", "block_default_avatar", "avatar", v.block_default_avatar)}
            {toggle("verification", "block_name_patterns", "names", v.block_name_patterns)}
            {toggle("verification", "dm_on_block", "dm", v.dm_on_block)}
          </Panel>
          <Panel title={t("raid")}>
            {toggle("raid", "enabled", "enabled", r.enabled)}
            <p className="muted">{t("raid_hint")}</p>
            <div className="safety-fields">
              {number("raid", "join_threshold", "threshold", r.join_threshold, 2, 500)}
              {number("raid", "window_secs", "window", r.window_secs, 5, 600)}
              {number("raid", "similar_threshold", "similar", r.similar_threshold, 2, 500)}
              {number("raid", "new_account_hours", "young", r.new_account_hours, 0, 8760)}
            </div>
            {toggle("raid", "similar_names", "similar_names", r.similar_names)}
            {toggle("raid", "similar_avatars", "similar_avatars", r.similar_avatars)}
            <div className="safety-fields">
              {select("raid", "action", "action", r.action, ["alert", "lockdown", "kick", "ban"])}
              {number("raid", "lockdown_minutes", "duration", r.lockdown_minutes, 1, 1440)}
            </div>
            <Field
              label={t("channels")}
              htmlFor="safety-channels"
              aside={t("used", { count: r.channels.length, max: 25 })}
            >
              <select
                id="safety-channels"
                className="pixel-input safety-multi"
                multiple
                value={r.channels}
                onChange={(e) =>
                  change(
                    "raid",
                    "channels",
                    Array.from(e.target.selectedOptions, (o) => o.value),
                  )
                }
              >
                {discord.data?.channels
                  .filter((ch) => [0, 5].includes(ch.type))
                  .map((ch) => (
                    <option key={ch.id} value={ch.id}>
                      #{ch.name}
                    </option>
                  ))}
              </select>
            </Field>
            {number("raid", "slowmode_secs", "slowmode", r.slowmode_secs, 0, 21600)}
            {toggle("raid", "lock_channels", "lock", r.lock_channels)}
            {toggle("raid", "pause_invites", "invites", r.pause_invites)}
          </Panel>
        </div>
        <aside className="safety-side">
          <Panel title={t("message")}>
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
                onChange={(e) => change("verification", "message", e.target.value)}
              />
            </Field>
            <PixelButton
              variant="secondary"
              disabled={post.isPending || dirty || !v.channel_id}
              onClick={() => post.mutate()}
            >
              {t("post")}
            </PixelButton>
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
          </Panel>
          <Panel title={t("exempt_roles")}>
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
                  change(
                    "verification",
                    "exempt_roles",
                    Array.from(e.target.selectedOptions, (o) => o.value),
                  )
                }
              >
                {discord.data?.roles.map((role) => (
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
                  change("verification", "exempt_users", idList(e.target.value));
                }}
              />
            </Field>
            <Field label={t("alert_channel")} htmlFor="safety-alert">
              <ChannelPicker
                id="safety-alert"
                info={discord.data}
                value={r.alert_channel_id}
                none="—"
                onChange={(id) => change("raid", "alert_channel_id", id)}
              />
            </Field>
            <Field label={t("log_channel")} htmlFor="safety-log">
              <ChannelPicker
                id="safety-log"
                info={discord.data}
                value={v.log_channel_id}
                none="—"
                onChange={(id) => change("verification", "log_channel_id", id)}
              />
            </Field>
          </Panel>
        </aside>
      </div>
      <Panel title={t("joins")}>
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
      </Panel>
      <Panel title={t("events")}>
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
      </Panel>
      <SafetySaveBar
        dirty={dirty}
        saving={save.isPending}
        t={t}
        onSave={() => save.mutate(c, { onSuccess: () => setDraft(null) })}
        onReset={() => {
          setDraft(null);
          setUsersText(null);
          save.reset();
        }}
      />
    </SettingsPage>
  );
}
