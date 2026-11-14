import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { ApiError, type LogSettings, type Overview } from "../../../lib/api";
import { type LogDraft, logDraft, logsPatch } from "../api/moderation";
import { useDiscord, useLogs } from "../../../lib/queries";
import { LogCategories } from "./log/LogCategories";
import { IgnoreLists } from "./log/IgnoreLists";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

export function ServerLog() {
  const data = useOutletContext<Overview>();
  const { query } = useLogs(data.guild.id);
  return (
    <SettingsPage
      className="settings-page mod-page"
      title={deferredText("SERVER LOG")}
      lead={deferredText(
        "A channel (or one per category) where the bot notes who came and went, what was edited or deleted, and what staff changed.",
      )}
    >
      {query.isPending && <Loading label={deferredText("Reading the log settings")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <LogForm guild={data.guild.id} saved={query.data} />}
    </SettingsPage>
  );
}

function LogForm({ guild, saved }: { guild: string; saved: LogSettings }) {
  const discord = useDiscord(guild);
  const { save } = useLogs(guild);
  const [draft, setDraft] = useState<LogDraft>(() => logDraft(saved));
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const { patch, count } = logsPatch(saved, draft);
  const error = save.error instanceof ApiError ? save.error : null;
  const fieldError = (field: string) => error?.errors.find((e) => e.field === field)?.message ?? null;
  const on = draft.channel_id !== null || Object.values(draft.category_channels).some((c) => c !== null);
  const set = (next: Partial<LogDraft>) => setDraft((d) => ({ ...d, ...next }));

  const submit = () =>
    save.mutate(patch, {
      onSuccess: (now) => {
        setDraft(logDraft(now));
        setToast(deferredText("Saved. The log follows it from the next event."));
      },
    });
  const reset = () => {
    setDraft(logDraft(saved));
    save.reset();
  };

  return (
    <>
      <Panel title={deferredText("LOG CHANNEL")}>
        <Field
          label={spaTranslate("Channel")}
          htmlFor="log-channel"
          error={fieldError("channel_id")}
          hint={deferredText(
            "Picking a channel turns the log on. Keep it private to moderators. A category below can have a channel of its own.",
          )}
        >
          <ChannelPicker
            posting
            id="log-channel"
            info={discord.data}
            value={draft.channel_id}
            onChange={(id) => set({ channel_id: id })}
            none={deferredText("Off: no server log")}
            invalid={!!fieldError("channel_id")}
          />
        </Field>
      </Panel>

      <LogCategories
        categories={saved.categories}
        draft={draft}
        set={set}
        info={discord.data}
        on={on}
        fieldError={fieldError}
      />
      <IgnoreLists
        draft={draft}
        set={set}
        maxIgnored={saved.max_ignored}
        info={discord.data}
        fieldError={fieldError}
      />

      <SaveBar
        count={count}
        saving={save.isPending}
        error={save.isError && !error?.errors.length ? save.error.message : null}
        onSave={submit}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}
