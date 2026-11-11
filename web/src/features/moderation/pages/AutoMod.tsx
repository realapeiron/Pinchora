import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";
import { Field } from "../../../components/Field";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useAutoMod, words, type AutoConfig, type AutoData, type AutoRule } from "../api/automod";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { SettingsRow } from "../../../components/SettingsRow";
import { dashboardText } from "./automodSummary";
import { automodSummary } from "./automodSummary";
import { RuleCard } from "./AutoModRule";
import { AutoModTest, AutoModHits } from "./AutoModActivity";
import { AutoModProfile } from "./AutoModProfile";
import { AutoModSettings } from "./AutoModSettings";
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
  const summary = automodSummary(config, saved, t);
  const observe = Boolean(config.observe_until && Date.parse(config.observe_until) > Date.now());
  return (
    <SettingsPage className="settings-page automod-page" title={t("ui.title")} lead={t("ui.lead")}>
      <div className="settings-detail-list">
        <AutoModProfile
          config={config}
          t={t}
          profile={profile}
          pending={pending}
          observe={observe}
          summary={summary.profile}
        />
        {!saved.message_content && (
          <p className="notice" role="status">
            {t("content_locked")}
          </p>
        )}
        <SettingsRow
          title={t("ui.rules")}
          description={dashboardText("Review and edit message rules.")}
          summary={summary.rules}
        >
          <div className="settings-detail-fields">
            <div className="automod-section-head">
              <h2 className="pixel-title">{t("ui.rules")}</h2>
              <span className="muted">
                {t("ui.used", { count: config.rules.length, max: saved.max_rules })}
              </span>
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
                          {
                            ...structuredClone(template),
                            name: `${kind} ${n}`,
                            enabled: false,
                            native: false,
                          },
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
          </div>
        </SettingsRow>
        <AutoModSettings
          config={config}
          saved={saved}
          t={t}
          info={discord.data}
          setConfig={setConfig}
          summary={summary.settings}
        />
        <SettingsRow
          title={t("ui.dm_on_action")}
          description={dashboardText("Send members a DM after an action.")}
          summary={summary.dm}
          enabled={config.dm_on_action}
          onToggle={(v) => setConfig((c) => ({ ...c, dm_on_action: v }))}
        >
          <p className="muted">{t("ui.native_hint")}</p>
        </SettingsRow>
        <AutoModTest
          t={t}
          summary={summary.test}
          info={discord.data}
          test={test}
          text={text}
          setText={setText}
          testChannel={testChannel}
          setTestChannel={setTestChannel}
          config={config}
        />
        <AutoModHits
          t={t}
          summary={summary.hits}
          saved={saved}
          config={config}
          reset={reset}
          setToast={setToast}
        />
      </div>
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
