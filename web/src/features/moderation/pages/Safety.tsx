import { dateTime } from "../../../i18n";
import { useState } from "react";
import { useOutletContext } from "react-router";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useSafety, words, type SafetyData, type SafetyConfig } from "../api/verification";
import { PixelButton } from "../../../components/PixelButton";
import { SafetyChecks } from "./SafetyChecks";
import { SafetyMessage, SafetyExemptions, SafetyJoins, SafetyEvents } from "./SafetyRows";
import { SafetySaveBar } from "./SafetySaveBar";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { safetySummary } from "./safetySummary";
import "../../../styles/verification.css";
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
  const summary = safetySummary(c, saved, t);
  const active = saved.events.find((e) => !e.ended_at);
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
      <div className="settings-detail-list safety-columns">
        <SafetyChecks
          v={v}
          r={r}
          t={t}
          info={discord.data}
          summary={summary}
          onChange={(section, key, value) => change(section, key as never, value)}
        />
        <aside className="safety-side">
          <SafetyMessage
            v={v}
            t={t}
            summary={summary.message}
            dirty={dirty}
            post={post}
            preview={preview}
            onChange={(section, key, value) => change(section, key as never, value)}
          />
          <SafetyExemptions
            v={v}
            r={r}
            t={t}
            summary={summary.exemptions}
            discord={discord.data}
            usersText={usersText}
            setUsersText={setUsersText}
            onChange={(section, key, value) => change(section, key as never, value)}
          />
        </aside>
      </div>
      <div className="settings-detail-list">
        <SafetyJoins saved={saved} t={t} summary={summary.joins} />
        <SafetyEvents saved={saved} t={t} summary={summary.events} />
      </div>
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
