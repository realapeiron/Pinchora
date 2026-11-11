import { Field, SwitchRow } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { SettingsRow } from "../../../components/SettingsRow";
import type { GuildInfo } from "../../../lib/api";
import { contentLocked, moveAction, type AutoRule, type Words } from "../api/automod";
import { ActionEditor, IdList } from "./AutoModControls";
import { automodRuleSummary } from "./automodSummary";

export function RuleCard({
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
    <SettingsRow
      title={`${t(`kind.${r.kind}`)}: ${r.name}`}
      description={r.discord_rule_id ? t("ui.native") : t("ui.local")}
      summary={automodRuleSummary(r, t)}
      enabled={r.enabled}
      onToggle={(enabled) => patch({ enabled })}
      toggleDisabled={locked || (scoped && !r.only_channels.length)}
      toggleLabel={`${t("ui.enabled")}: ${t(`kind.${r.kind}`)} (${r.name})`}
    >
      <div className="settings-detail-fields automod-rule">
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
      </div>
    </SettingsRow>
  );
}
