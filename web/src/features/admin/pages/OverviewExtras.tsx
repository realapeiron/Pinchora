import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { t } from "../../../i18n/dashboard";
import type { Overview, Settings } from "../../../lib/api";
import { changedFields, patchOf } from "../../../lib/draft";
import { useNames, useSaveSettings } from "../../../lib/queries";
import { DEFAULTS, LIMITS, checkPrefix } from "../../../lib/rules";
import { describeChange, timeAgo } from "../../../lib/text";

// Translation keys retained from the earlier overview for saved audit wording and older clients:
// OVERVIEW; What the bot uses right now; Chat XP; TEXT COMMANDS; no channel yet;
// level-ups in their channel; level-ups where they chat; level-ups by DM;
// no level-up messages; {theme} theme; {emoji} {min}–{max} {name} per message;
// nothing picked; Members earn XP for chatting, climb levels and get a level-up card. Turn it off to pause XP in this server; levels already earned stay.

export function OverviewExtras({ data }: { data: Overview }) {
  const guild = data.guild.id;
  const [draft, setDraft] = useState<Partial<Settings>>({});
  const [toast, setToast] = useState<string | null>(null);
  const save = useSaveSettings(guild);
  const names = useNames(guild);
  const prefix = draft.prefix ?? data.settings.prefix;
  const valid = checkPrefix(prefix);
  const error = draft.prefix !== undefined && !valid.ok ? valid.message : null;
  return (
    <div className="overview-extras">
      <Panel title={t("Quick settings")}>
        <Field
          label={t("Prefix")}
          htmlFor="text-prefix"
          error={error}
          hint={deferredText("Every command also works as text: {arg1}profile, or @Pinchora profile.", {
            arg1: prefix.trim() || DEFAULTS.prefix,
          })}
          aside={
            prefix !== DEFAULTS.prefix && (
              <PixelButton
                variant="ghost"
                small
                disabled={save.isPending}
                onClick={() => setDraft((old) => ({ ...old, prefix: DEFAULTS.prefix }))}
              >
                {t("Default")}
              </PixelButton>
            )
          }
        >
          <input
            id="text-prefix"
            className="pixel-input input-short"
            value={prefix}
            maxLength={LIMITS.prefix_max + 8}
            aria-invalid={!!error}
            disabled={save.isPending}
            onChange={(event) => setDraft({ prefix: event.target.value })}
          />
        </Field>
      </Panel>
      <Panel title={deferredText("RECENT CHANGES")}>
        {data.audit.length === 0 ? (
          <p className="muted">
            {deferredText("No changes from the dashboard yet. What you save here shows up in this list.")}
          </p>
        ) : (
          <ol className="audit-list">
            {data.audit.map((entry) => (
              <li key={entry.id} className="audit-row">
                <span className="audit-dot" aria-hidden="true" />
                <span className="audit-text">
                  <strong>{entry.actor_name}</strong> {describeChange(entry, names)}
                  {entry.source === "discord" && <span className="muted"> {t("in Discord")}</span>}
                </span>
                <time className="audit-time muted" dateTime={entry.created_at}>
                  {timeAgo(entry.created_at)}
                </time>
              </li>
            ))}
          </ol>
        )}
      </Panel>
      <SaveBar
        count={changedFields(data.settings, draft).length}
        saving={save.isPending}
        error={save.isError ? save.error.message : null}
        onReset={() => {
          setDraft({});
          save.reset();
        }}
        onSave={() => {
          if (!error)
            save.mutate(patchOf(data.settings, draft), {
              onSuccess: () => {
                setDraft({});
                setToast(t("Saved. The bot uses it from the next message."));
              },
            });
        }}
      />
      <Toast message={toast} onDone={() => setToast(null)} />
    </div>
  );
}
