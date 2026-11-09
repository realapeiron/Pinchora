import { useState } from "react";
import { dateTime, number } from "../../../i18n";
import { useOutletContext } from "react-router";
import { Field, SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { SaveBar } from "../../../components/SaveBar";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import {
  draftProblem,
  validUserId,
  useAntiNuke,
  words,
  type AntiNukeData,
  type AntiNukeConfig,
} from "../api/anti_nuke";
import { SettingsPage } from "../../shell/pages/PageHead";
import { Loading, Problem } from "../../shell/pages/States";
import "../../../styles/anti_nuke.css";

export function AntiNuke() {
  const overview = useOutletContext<Overview>();
  const { query } = useAntiNuke(overview.guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Editor key={JSON.stringify(query.data.config)} guild={overview.guild.id} saved={query.data} />;
}
function Editor({ guild, saved }: { guild: string; saved: AntiNukeData }) {
  const t = words(saved.strings, saved.language);
  const [config, setConfig] = useState(saved.config);
  const [user, setUser] = useState("");
  const [role, setRole] = useState<string | null>(null);
  const [kind, setKind] = useState("channel_delete");
  const [count, setCount] = useState(6);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const { save, preview, restore } = useAntiNuke(guild);
  const discord = useDiscord(guild);
  const dirty = JSON.stringify(config) !== JSON.stringify(saved.config);
  const locked = !saved.can_edit || save.isPending;
  const set = <K extends keyof AntiNukeConfig>(key: K, value: AntiNukeConfig[K]) =>
    setConfig((c) => ({ ...c, [key]: value }));
  const roleName = (id: string) => discord.data?.roles.find((r) => r.id === id)?.name ?? id;
  const toggle = (
    key: "enabled" | "auto_restore" | "revert_vip_bans" | "remove_bots" | "owner_only_config",
    label: string,
    disabled = locked,
  ) => (
    <SwitchRow
      key={key}
      title={t(label)}
      control={
        <PixelToggle
          label={t(label)}
          checked={config[key]}
          disabled={disabled}
          onChange={(v) => set(key, v)}
        />
      }
    />
  );
  return (
    <SettingsPage className="settings-page antinuke-page" title={t("ui.title")} lead={t("ui.lead")}>
      {!saved.can_edit && <p className="notice">{t("ui.readonly")}</p>}
      <Panel
        title={t("ui.protection")}
        aside={
          <span className={`antinuke-state ${config.enabled ? "is-on" : ""}`}>
            {t(config.enabled ? "on" : "off")}
          </span>
        }
      >
        {toggle("enabled", "ui.enabled")}
        <div className="antinuke-settings-grid">
          <Field label={t("ui.response")} htmlFor="an-response">
            <select
              id="an-response"
              className="pixel-input pixel-select"
              value={config.response}
              disabled={locked}
              onChange={(e) => set("response", e.target.value)}
            >
              {saved.responses.map((r) => (
                <option key={r} value={r}>
                  {t(`response.${r}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("ui.alert_channel")} htmlFor="an-alert">
            <ChannelPicker
              id="an-alert"
              info={discord.data}
              value={config.log_channel_id}
              none={t("ui.fallback")}
              disabled={locked}
              onChange={(v) => set("log_channel_id", v)}
            />
          </Field>
          {config.response === "quarantine" && (
            <Field label={t("ui.quarantine")} htmlFor="an-quarantine">
              <RolePicker
                id="an-quarantine"
                info={discord.data}
                value={config.quarantine_role_id}
                none={t("ui.none")}
                disabled={locked}
                onChange={(v) => set("quarantine_role_id", v)}
              />
            </Field>
          )}
        </div>
        {toggle("auto_restore", "ui.auto_restore")}
        {toggle("revert_vip_bans", "ui.revert_vip")}
        {toggle("remove_bots", "ui.remove_bots")}
        <p className="muted antinuke-guidance">{t("ui.recovery_hint")}</p>
      </Panel>
      <Panel title={t("ui.limits")}>
        <p className="muted antinuke-guidance">{t("ui.limit_hint")}</p>
        <div className="antinuke-limits">
          {saved.kinds
            .filter((k) => !k.startsWith("vip_"))
            .map((k) => {
              const limit = config.limits[k]!;
              return (
                <div className="antinuke-limit" key={k}>
                  <h3>{t(`kind.${k}`)}</h3>
                  <Field label={t("ui.count")} htmlFor={`an-count-${k}`}>
                    <div className="antinuke-slider">
                      <input
                        id={`an-count-${k}`}
                        aria-label={`${t(`kind.${k}`)} · ${t("ui.count")}`}
                        type="range"
                        min="1"
                        max="100"
                        value={limit.count}
                        disabled={locked}
                        onChange={(e) =>
                          set("limits", {
                            ...config.limits,
                            [k]: { ...limit, count: Number(e.target.value) },
                          })
                        }
                      />
                      <output htmlFor={`an-count-${k}`}>{limit.count}</output>
                    </div>
                  </Field>
                  <Field label={t("ui.seconds")} htmlFor={`an-window-${k}`}>
                    <input
                      id={`an-window-${k}`}
                      aria-label={`${t(`kind.${k}`)} · ${t("ui.seconds")}`}
                      className="pixel-input"
                      type="number"
                      min="5"
                      max="300"
                      value={limit.window_secs}
                      disabled={locked}
                      onChange={(e) =>
                        set("limits", {
                          ...config.limits,
                          [k]: { ...limit, window_secs: Number(e.target.value) },
                        })
                      }
                    />
                  </Field>
                </div>
              );
            })}
        </div>
      </Panel>
      <div className="antinuke-people-grid">
        <Panel
          title={t("ui.trust")}
          aside={
            <span className="muted">
              {t("ui.used", { count: config.trusted_users.length, max: saved.max_list })}
            </span>
          }
        >
          <p className="muted">{t("ui.trust_hint")}</p>
          {toggle("owner_only_config", "ui.owner_only", !saved.can_trust || save.isPending)}
          <div className="antinuke-add">
            <Field label={t("ui.member")} htmlFor="an-user">
              <input
                id="an-user"
                className="pixel-input"
                inputMode="numeric"
                autoComplete="off"
                value={user}
                disabled={!saved.can_trust}
                onChange={(e) => setUser(e.target.value.trim())}
              />
            </Field>
            <PixelButton
              variant="secondary"
              disabled={
                !saved.can_trust ||
                save.isPending ||
                !validUserId(user, saved.viewer) ||
                config.trusted_users.includes(user) ||
                config.trusted_users.length >= saved.max_list
              }
              onClick={() => {
                set("trusted_users", [...config.trusted_users, user]);
                setUser("");
              }}
            >
              {t("ui.add")}
            </PixelButton>
          </div>
          <ul className="antinuke-members">
            {config.trusted_users.map((id) => (
              <li key={id}>
                <code>{id}</code>
                <PixelButton
                  variant="ghost"
                  disabled={!saved.can_trust || save.isPending}
                  aria-label={`${t("ui.remove")} ${id}`}
                  onClick={() =>
                    set(
                      "trusted_users",
                      config.trusted_users.filter((u) => u !== id),
                    )
                  }
                >
                  {t("ui.remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        </Panel>
        <Panel
          title={t("ui.vip")}
          aside={
            <span className="muted">
              {t("ui.used", { count: config.vip_roles.length, max: saved.max_list })}
            </span>
          }
        >
          <p className="muted">{t("ui.vip_hint")}</p>
          <div className="antinuke-add">
            <Field label={t("ui.add_role")} htmlFor="an-vip">
              <RolePicker
                id="an-vip"
                info={discord.data}
                value={role}
                none={t("ui.none")}
                disabled={locked}
                onChange={setRole}
              />
            </Field>
            <PixelButton
              variant="secondary"
              disabled={
                locked ||
                !role ||
                config.vip_roles.includes(role) ||
                config.vip_roles.length >= saved.max_list
              }
              onClick={() => {
                if (role) set("vip_roles", [...config.vip_roles, role]);
                setRole(null);
              }}
            >
              {t("ui.add_role")}
            </PixelButton>
          </div>
          <ul className="antinuke-members">
            {config.vip_roles.map((id) => (
              <li key={id}>
                <span>{roleName(id)}</span>
                <PixelButton
                  variant="ghost"
                  disabled={locked}
                  aria-label={`${t("ui.remove")} ${roleName(id)}`}
                  onClick={() =>
                    set(
                      "vip_roles",
                      config.vip_roles.filter((r) => r !== id),
                    )
                  }
                >
                  {t("ui.remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        </Panel>
      </div>
      <Panel title={t("ui.preview")}>
        <p className="muted">{t("ui.preview_hint")}</p>
        <div className="antinuke-preview">
          <Field label={t("ui.kind")} htmlFor="an-preview-kind">
            <select
              id="an-preview-kind"
              className="pixel-input pixel-select"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              {saved.kinds.map((k) => (
                <option key={k} value={k}>
                  {t(`kind.${k}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("ui.preview_count")} htmlFor="an-preview-count">
            <input
              id="an-preview-count"
              className="pixel-input"
              type="number"
              min="1"
              max="1000"
              value={count}
              onChange={(e) => setCount(Number(e.target.value))}
            />
          </Field>
          <PixelButton
            variant="secondary"
            disabled={preview.isPending || draftProblem(config) || count < 1 || count > 1000}
            onClick={() => preview.mutate({ config, kind, count })}
          >
            {t("ui.run_preview")}
          </PixelButton>
        </div>
        {preview.data && (
          <p className="notice" role="status">
            {preview.data.message}
          </p>
        )}
        {preview.isError && (
          <p className="field-error" role="alert">
            {preview.error.message}
          </p>
        )}
      </Panel>
      <Panel title={t("ui.events")}>
        <p className="muted">{t("ui.events_hint")}</p>
        {saved.events.length === 0 ? (
          <p className="empty-row muted">{t("empty")}</p>
        ) : (
          <ol className="antinuke-events">
            {saved.events.map((event) => (
              <li key={event.id}>
                <div className="antinuke-event-head">
                  <h3>
                    {t(`kind.${event.kind}`)} <span className="muted">×{number(event.count)}</span>
                  </h3>
                  <time dateTime={event.created_at}>{dateTime(event.created_at)}</time>
                </div>
                <p>
                  <span className="muted">{t("ui.actor")}: </span>
                  <code>{event.actor_id}</code>
                </p>
                <p>{event.action}</p>
                {event.reverted ? (
                  <p className="antinuke-recovered">{t("ui.restored")}</p>
                ) : (
                  event.restorable &&
                  saved.can_restore && (
                    <PixelButton
                      variant="secondary"
                      disabled={restore.isPending}
                      onClick={() => setConfirm(event.id)}
                    >
                      {t("undo")}
                    </PixelButton>
                  )
                )}
                {confirm === event.id && (
                  <div className="antinuke-confirm" role="group" aria-label={t("ui.confirm")}>
                    <p>{t("ui.confirm_hint")}</p>
                    <div className="antinuke-confirm-actions">
                      <PixelButton
                        disabled={restore.isPending}
                        onClick={() =>
                          restore.mutate(event.id, {
                            onSuccess: (data) => {
                              setNotice(data.message);
                              setConfirm(null);
                            },
                          })
                        }
                      >
                        {t("ui.confirm")}
                      </PixelButton>
                      <PixelButton
                        variant="ghost"
                        disabled={restore.isPending}
                        onClick={() => setConfirm(null)}
                      >
                        {t("ui.cancel")}
                      </PixelButton>
                    </div>
                  </div>
                )}
              </li>
            ))}
          </ol>
        )}
        {notice && (
          <p role="status" className="notice">
            {notice}
          </p>
        )}
        {restore.isError && (
          <p className="field-error" role="alert">
            {restore.error.message}
          </p>
        )}
      </Panel>
      <SaveBar
        words={(key, values) => t(`ui.${key}`, values)}
        count={dirty ? 1 : 0}
        saving={save.isPending}
        error={draftProblem(config) ? t("invalid") : save.error?.message}
        blocked={locked || draftProblem(config)}
        onSave={() => save.mutate(config, { onSuccess: () => setNotice(t("saved")) })}
        onReset={() => {
          setConfig(saved.config);
          save.reset();
        }}
      />
    </SettingsPage>
  );
}
