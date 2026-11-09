import { dateTime } from "../../../i18n";
import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";
import { Field, SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker, RolePicker } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { GuildInfo, Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import {
  actionLabel,
  contentLocked,
  moveAction,
  useAutoMod,
  words,
  type AutoAction,
  type AutoConfig,
  type AutoData,
  type AutoRule,
  type Words,
} from "../api/automod";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/automod.css";

export function AutoMod() {
  const overview = useOutletContext<Overview>();
  const { query } = useAutoMod(overview.guild.id);
  if (query.isPending) return <Loading />;
  if (query.isError) return <Problem error={query.error} />;
  return <Editor key={JSON.stringify(query.data.config)} guild={overview.guild.id} saved={query.data} />;
}
function Editor({ guild, saved }: { guild: string; saved: AutoData }) {
  const t = words(saved.strings);
  const [config, setConfig] = useState<AutoConfig>(saved.config);
  const [text, setText] = useState("");
  const [testChannel, setTestChannel] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const { save, profile, test, reset } = useAutoMod(guild);
  const discord = useDiscord(guild);
  const changeRule = (index: number, rule: AutoRule) =>
    setConfig((c) => ({ ...c, rules: c.rules.map((r, i) => (i === index ? rule : r)) }));
  const changed = JSON.stringify(saved.config) !== JSON.stringify(config);
  const pending = save.isPending || profile.isPending;
  const observe = config.observe_until && Date.parse(config.observe_until) > Date.now();
  return (
    <SettingsPage className="settings-page automod-page" title={t("ui.title")} lead={t("ui.lead")}>
      <Panel title={t("ui.profiles")}>
        <div className="automod-profiles">
          {["relaxed", "standard", "strict", "off"].map((p) => (
            <button
              key={p}
              className="automod-profile"
              type="button"
              aria-pressed={config.profile === p}
              disabled={pending}
              onClick={() => profile.mutate(p)}
            >
              <span className="pixel-label">{t(`ui.${p}`)}</span>
              <span>{t(`ui.${p}_hint`)}</span>
            </button>
          ))}
        </div>
        <p className="automod-observation">{t("ui.observe")}</p>
        {observe && (
          <p className="automod-until">
            {t("ui.observing_until", { time: dateTime(config.observe_until!) })}
          </p>
        )}
        {profile.isError && (
          <p role="alert" className="field-error">
            {profile.error.message}
          </p>
        )}
      </Panel>
      {!saved.message_content && (
        <p className="notice" role="status">
          {t("content_locked")}
        </p>
      )}
      <div className="automod-section-head">
        <h2 className="pixel-title">{t("ui.rules")}</h2>
        <span className="muted">{t("ui.used", { count: config.rules.length, max: saved.max_rules })}</span>
      </div>
      {config.rules.length === 0 && <p className="empty-row muted">{t("ui.no_rules")}</p>}
      <div className="automod-rules">
        {config.rules.map((r, i) => (
          <RuleCard
            key={r.id ?? `${i}-${r.kind}`}
            rule={r}
            t={t}
            info={discord.data}
            content={saved.message_content}
            actions={saved.actions}
            onChange={(r) => changeRule(i, r)}
            onRemove={() => setConfig((c) => ({ ...c, rules: c.rules.filter((_, n) => n !== i) }))}
          />
        ))}
      </div>
      <div className="automod-add">
        <Field label={t("ui.add_rule")} htmlFor="am-add-kind">
          <select
            id="am-add-kind"
            className="pixel-input pixel-select"
            value=""
            disabled={config.rules.length >= saved.max_rules}
            onChange={(e) => {
              const kind = e.target.value;
              const template = saved.profiles.standard?.rules.find((r) => r.kind === kind);
              if (template) {
                let n = 1;
                while (config.rules.some((r) => r.name === `${kind} ${n}`)) n++;
                setConfig((c) => ({
                  ...c,
                  rules: [
                    ...c.rules,
                    { ...structuredClone(template), name: `${kind} ${n}`, enabled: false, native: false },
                  ],
                }));
              }
            }}
          >
            <option value="">{t("ui.pick")}</option>
            {saved.kinds.map((k) => (
              <option key={k} value={k}>
                {t(`kind.${k}`)}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <Panel title={t("ui.settings")}>
        <Field label={t("ui.alert_channel")} htmlFor="am-alert">
          <ChannelPicker
            id="am-alert"
            info={discord.data}
            value={config.alert_channel_id}
            onChange={(id) => setConfig((c) => ({ ...c, alert_channel_id: id }))}
            none={t("ui.mod_channel")}
          />
        </Field>
        <SwitchRow
          title={t("ui.dm_on_action")}
          control={
            <PixelToggle
              label={t("ui.dm_on_action")}
              checked={config.dm_on_action}
              onChange={(v) => setConfig((c) => ({ ...c, dm_on_action: v }))}
            />
          }
        >
          {t("ui.native_hint")}
        </SwitchRow>
        <details className="automod-details">
          <summary>{t("ui.escalation")}</summary>
          <p className="muted">{t("ui.escalation_hint")}</p>
          <Field label={t("ui.window")} htmlFor="am-escalation-window">
            <input
              id="am-escalation-window"
              type="number"
              min="1"
              max="2592000"
              className="pixel-input"
              value={config.escalation[0]?.window_secs ?? 3600}
              onChange={(e) =>
                setConfig((c) => ({
                  ...c,
                  escalation: c.escalation.map((step) => ({ ...step, window_secs: Number(e.target.value) })),
                }))
              }
            />
          </Field>
          {config.escalation.map((step, i) => (
            <div className="automod-escalation" key={i}>
              <Field label={t("ui.strikes")} htmlFor={`am-strikes-${i}`}>
                <input
                  id={`am-strikes-${i}`}
                  type="number"
                  min="1"
                  max="100"
                  className="pixel-input"
                  value={step.strikes}
                  onChange={(e) =>
                    setConfig((c) => ({
                      ...c,
                      escalation: c.escalation.map((s, n) =>
                        n === i ? { ...s, strikes: Number(e.target.value) } : s,
                      ),
                    }))
                  }
                />
              </Field>
              <ActionEditor
                action={step.action}
                actions={saved.actions}
                info={discord.data}
                t={t}
                onChange={(action) =>
                  setConfig((c) => ({
                    ...c,
                    escalation: c.escalation.map((s, n) => (n === i ? { ...s, action } : s)),
                  }))
                }
              />
              <PixelButton
                variant="secondary"
                onClick={() =>
                  setConfig((c) => ({ ...c, escalation: c.escalation.filter((_, n) => n !== i) }))
                }
              >
                {t("ui.remove")}
              </PixelButton>
            </div>
          ))}
          <PixelButton
            variant="secondary"
            disabled={config.escalation.length >= 10}
            onClick={() =>
              setConfig((c) => ({
                ...c,
                escalation: [
                  ...c.escalation,
                  {
                    strikes: Math.max(0, ...c.escalation.map((s) => s.strikes)) + 3,
                    window_secs: c.escalation[0]?.window_secs ?? 3600,
                    action: { kind: "timeout", secs: 600, role: null },
                  },
                ],
              }))
            }
          >
            {t("ui.add_step")}
          </PixelButton>
        </details>
        <p className="muted">
          {t("ui.sync")}:{" "}
          {saved.sync_error ?? (saved.synced_at ? dateTime(saved.synced_at) : t("ui.sync_pending"))}
        </p>
      </Panel>
      <Panel title={t("ui.test")}>
        <Field label={t("ui.test_channel")} htmlFor="am-test-channel">
          <ChannelPicker
            id="am-test-channel"
            info={discord.data}
            value={testChannel}
            onChange={setTestChannel}
            none={t("ui.pick")}
          />
        </Field>
        <p className="muted panel-lead">{t("ui.test_hint")}</p>
        <Field label={t("ui.test_text")} htmlFor="am-test">
          <textarea
            id="am-test"
            className="pixel-input"
            rows={3}
            maxLength={2000}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        </Field>
        <PixelButton
          disabled={!text.trim() || test.isPending}
          onClick={() => test.mutate({ text, config, channel_id: testChannel })}
        >
          {t("ui.run_test")}
        </PixelButton>
        {test.isError && (
          <p className="field-error" role="alert">
            {test.error.message}
          </p>
        )}
        {test.data && (
          <div className="automod-test-result" role="status">
            {test.data.matches.length === 0 ? (
              <p>{t("no_matches")}</p>
            ) : (
              <ul>
                {test.data.matches.map((r, i) => (
                  <li key={i}>
                    <strong>{t(`kind.${r.kind}`)}</strong> ·{" "}
                    {r.log_only
                      ? t("ui.dry_result")
                      : r.actions.map((a) => t(`action.${a.kind}`)).join(" → ")}
                  </li>
                ))}
              </ul>
            )}
            <p className="muted">{test.data.note}</p>
          </div>
        )}
      </Panel>
      <Panel title={t("ui.hits")} aside={<span className="muted">{saved.hits.length}/50</span>}>
        <p className="muted panel-lead">{t("ui.hits_hint")}</p>
        {saved.hits.length === 0 ? (
          <p className="automod-empty">{t("ui.empty_hits")}</p>
        ) : (
          <ul className="automod-hits">
            {saved.hits.map((h) => (
              <li key={h.id}>
                <div>
                  <strong>{config.rules.find((r) => r.id === h.rule_id)?.name ?? `#${h.id}`}</strong>
                  <time dateTime={h.created_at}>{dateTime(h.created_at)}</time>
                </div>
                <p>
                  {t("ui.member")}: <code>{h.user_id}</code> · <code>#{h.channel_id}</code>
                </p>
                <p>{actionLabel(h.action, t, saved.actions)}</p>
                {h.snippet && <blockquote>{h.snippet}</blockquote>}
                <PixelButton
                  variant="secondary"
                  disabled={reset.isPending}
                  onClick={() => reset.mutate(h.user_id, { onSuccess: () => setToast(t("ui.saved")) })}
                >
                  {t("ui.reset")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        {reset.isError && <p role="alert">{reset.error.message}</p>}
      </Panel>
      <SaveBar
        count={changed ? 1 : 0}
        saving={pending}
        error={save.isError ? save.error.message : null}
        onSave={() => save.mutate(config)}
        onReset={() => {
          setConfig(saved.config);
          save.reset();
        }}
      />
      <Toast message={toast} onDone={clearToast} />
    </SettingsPage>
  );
}
function RuleCard({
  rule: r,
  t,
  info,
  content,
  actions,
  onChange,
  onRemove,
}: {
  rule: AutoRule;
  t: Words;
  info: GuildInfo | undefined;
  content: boolean;
  actions: string[];
  onChange: (r: AutoRule) => void;
  onRemove: () => void;
}) {
  const locked = contentLocked(r, content);
  const patch = (patch: Partial<AutoRule>) => onChange({ ...r, ...patch });
  const params = (patch: Partial<AutoRule["params"]>) =>
    onChange({ ...r, params: { ...r.params, ...patch } });
  const scoped = ["honeypot", "media_only", "youtube_only"].includes(r.kind);
  return (
    <article className={`automod-rule ${r.enabled ? "is-enabled" : ""}`}>
      <header>
        <div>
          <h3>{t(`kind.${r.kind}`)}</h3>
          <span className="automod-badge">{r.discord_rule_id ? t("ui.native") : t("ui.local")}</span>
        </div>
        <PixelToggle
          label={`${t("ui.enabled")}: ${t(`kind.${r.kind}`)} (${r.name})`}
          checked={r.enabled}
          disabled={locked || (scoped && !r.only_channels.length)}
          onChange={(enabled) => patch({ enabled })}
        />
      </header>
      {locked && <p className="automod-lock">{t("ui.locked")}</p>}
      <p className="automod-chain">
        {r.log_only ? t("ui.dry_result") : r.actions.map((a) => t(`action.${a.kind}`)).join(" → ")}
      </p>
      <details className="automod-details">
        <summary>{t("ui.configure")}</summary>
        <Field label={t("ui.rule_name")}>
          <input
            aria-label={t("ui.rule_name")}
            className="pixel-input"
            maxLength={40}
            value={r.name}
            onChange={(e) => patch({ name: e.target.value })}
          />
        </Field>
        <div className="automod-pair">
          <Field label={t("ui.threshold")}>
            <input
              aria-label={t("ui.threshold")}
              type="number"
              min="1"
              max={r.kind === "caps" ? 100 : r.kind === "mentions" ? 50 : 10000}
              className="pixel-input"
              value={r.params.threshold}
              onChange={(e) => params({ threshold: Number(e.target.value) })}
            />
          </Field>
          <Field label={t("ui.window")}>
            <input
              aria-label={t("ui.window")}
              type="number"
              min="1"
              max="3600"
              className="pixel-input"
              value={r.params.window_secs}
              onChange={(e) => params({ window_secs: Number(e.target.value) })}
            />
          </Field>
        </div>
        <SwitchRow
          title={t("ui.log_only")}
          control={
            <PixelToggle
              label={t("ui.log_only")}
              checked={r.log_only}
              onChange={(log_only) => patch({ log_only })}
            />
          }
        />
        {["spam", "mentions", "bad_words", "invites", "links", "presets"].includes(r.kind) && (
          <SwitchRow
            title={t("ui.native_requested")}
            control={
              <PixelToggle
                label={t("ui.native_requested")}
                checked={r.native}
                onChange={(native) => patch({ native })}
              />
            }
          />
        )}
        {(
          [
            ...(r.kind === "bad_words" ? ["words", "regex"] : []),
            ...(r.kind === "links" ? ["allow_domains", "block_domains"] : []),
          ] as ("words" | "regex" | "allow_domains" | "block_domains")[]
        ).map((key) => (
          <Field key={key} label={t(`ui.${key}`)}>
            <textarea
              aria-label={t(`ui.${key}`)}
              className="pixel-input"
              rows={3}
              value={r.params[key].join("\n")}
              onChange={(e) => params({ [key]: e.target.value.split("\n").filter(Boolean) })}
            />
          </Field>
        ))}
        <h4>{t("ui.actions")}</h4>
        <ol className="automod-actions">
          {r.actions.map((a, i) => (
            <li key={i}>
              <ActionEditor
                action={a}
                actions={actions}
                info={info}
                t={t}
                onChange={(action) => patch({ actions: r.actions.map((a, n) => (n === i ? action : a)) })}
              />
              <div className="automod-action-tools">
                <button
                  type="button"
                  disabled={i === 0}
                  onClick={() => patch({ actions: moveAction(r.actions, i, -1) })}
                >
                  {t("ui.up")}
                </button>
                <button
                  type="button"
                  disabled={i === r.actions.length - 1}
                  onClick={() => patch({ actions: moveAction(r.actions, i, 1) })}
                >
                  {t("ui.down")}
                </button>
                <button
                  type="button"
                  disabled={r.actions.length === 1}
                  onClick={() => patch({ actions: r.actions.filter((_, n) => n !== i) })}
                >
                  {t("ui.remove")}
                </button>
              </div>
            </li>
          ))}
        </ol>
        <PixelButton
          variant="secondary"
          disabled={r.actions.length >= 8}
          onClick={() => patch({ actions: [...r.actions, { kind: "log", secs: 0, role: null }] })}
        >
          {t("ui.add_action")}
        </PixelButton>
        {(["exempt_roles", "exempt_channels", "only_channels"] as const).map((key) => (
          <IdList
            key={key}
            title={t(`ui.${key}`)}
            values={r[key]}
            roles={key === "exempt_roles"}
            info={info}
            t={t}
            onChange={(values) => patch({ [key]: values })}
          />
        ))}
        {scoped && <p className="muted">{t("ui.scope_hint")}</p>}
        <PixelButton variant="secondary" onClick={onRemove}>
          {t("ui.remove")}
        </PixelButton>
      </details>
    </article>
  );
}
function ActionEditor({
  action: a,
  actions,
  info,
  t,
  onChange,
}: {
  action: AutoAction;
  actions: string[];
  info: GuildInfo | undefined;
  t: Words;
  onChange: (a: AutoAction) => void;
}) {
  return (
    <div className="automod-action">
      <select
        aria-label={t("ui.actions")}
        className="pixel-input pixel-select"
        value={a.kind}
        onChange={(e) =>
          onChange({ ...a, kind: e.target.value, secs: e.target.value === "timeout" ? 600 : 0 })
        }
      >
        {actions.map((k) => (
          <option key={k} value={k}>
            {t(`action.${k}`)}
          </option>
        ))}
      </select>
      {a.kind === "timeout" && (
        <input
          aria-label={t("ui.duration")}
          type="number"
          className="pixel-input"
          min="1"
          max="2419200"
          value={a.secs}
          onChange={(e) => onChange({ ...a, secs: Number(e.target.value) })}
        />
      )}{" "}
      {a.kind === "add_role" && (
        <RolePicker
          info={info}
          value={a.role}
          onChange={(role) => onChange({ ...a, role })}
          none={t("ui.pick")}
          assignable
          forActions
        />
      )}
    </div>
  );
}
function IdList({
  title,
  values,
  roles,
  info,
  t,
  onChange,
}: {
  title: string;
  values: string[];
  roles: boolean;
  info: GuildInfo | undefined;
  t: Words;
  onChange: (values: string[]) => void;
}) {
  const add = (id: string | null) => {
    if (id && !values.includes(id)) onChange([...values, id]);
  };
  return (
    <div className="automod-id-list">
      <span className="field-label">
        {title}{" "}
        <span className="muted">
          ({values.length}/{roles ? 20 : 50})
        </span>
      </span>
      <div className="automod-chips">
        {values.map((id) => (
          <button
            type="button"
            className="channel-chip"
            key={id}
            aria-label={`${t("ui.remove")} ${id}`}
            onClick={() => onChange(values.filter((v) => v !== id))}
          >
            {roles ? "@" : "#"}
            {(roles ? info?.roles : info?.channels)?.find((r) => r.id === id)?.name ?? id} ×
          </button>
        ))}
      </div>
      {roles ? (
        <RolePicker
          info={info}
          value={null}
          onChange={add}
          none={t("ui.pick")}
          disabled={values.length >= 20}
        />
      ) : (
        <ChannelPicker
          label={title}
          info={info}
          value={null}
          onChange={add}
          none={t("ui.pick")}
          any
          disabled={values.length >= 50}
        />
      )}
    </div>
  );
}
