import { t as deferredText } from "../../../i18n/dashboard";
import { locale, translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Choice } from "../../../components/Choice";
import { Panel } from "../../../components/Panel";
import { SettingsRow } from "../../../components/SettingsRow";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { ChannelMode, GuildInfo, ModuleEntry, Modules as ModulesData, Overview } from "../../../lib/api";
import {
  type ModuleDraft,
  changeCount,
  effective,
  patchFor,
  problem,
  withChannels,
  withMode,
} from "../api/modules";
import { moduleSummary } from "./moduleSummary";
import { ModuleChannels } from "./ModuleChannels";
import { ModuleSaveError, useDiscord, useModules, useSaveModules } from "../../../lib/queries";
import { SettingsPage } from "../../../components/PageHead";
import { ModuleOffConfirm } from "./ModuleOffConfirm";
import { Loading, Problem } from "../../../components/States";

const MODES: { value: ChannelMode; title: string; detail: string }[] = [
  { value: "all", title: "All channels", detail: "Works everywhere in the server." },
  { value: "only", title: "Only in these", detail: "Works in the channels you pick, and nowhere else." },
  {
    value: "except",
    title: "Everywhere but these",
    detail: "Works everywhere except the channels you pick.",
  },
];

export function Modules() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const query = useModules(guild);
  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("MODULES")}
      lead={deferredText(
        "Switch parts of Pinchora off for the whole server, or keep them to some channels. Switched-off commands answer privately that the feature is off.",
      )}
    >
      {query.isPending && <Loading label={deferredText("Reading the modules")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <ModulesForm guild={guild} data={query.data} />}
    </SettingsPage>
  );
}

function ModulesForm({ guild, data }: { guild: string; data: ModulesData }) {
  const discord = useDiscord(guild);
  const save = useSaveModules(guild);
  const [drafts, setDrafts] = useState<Record<string, ModuleDraft>>({});
  const [toast, setToast] = useState<string | null>(null);
  const [confirmOff, setConfirmOff] = useState(false);
  const clearToast = useCallback(() => setToast(null), []);

  const changes = changeCount(data.items, drafts);
  const blocked = data.items.some((m) => problem(m, drafts[m.key], data.max_channels) !== null);
  const refused = save.error instanceof ModuleSaveError ? save.error : null;
  const edit = (key: string, next: ModuleDraft) => {
    save.reset();
    setDrafts((all) => ({ ...all, [key]: next }));
  };

  const submit = (approved = false) => {
    const todo = data.items.flatMap((saved) => {
      const patch = patchFor(saved, drafts[saved.key]);
      return patch ? [{ key: saved.key, patch }] : [];
    });
    if (!approved && todo.some(({ key, patch }) => key === "command_control" && patch.enabled === false)) {
      setConfirmOff(true);
      return;
    }
    save.mutate(todo, {
      onSuccess: () => {
        setDrafts({});
        setToast(deferredText("Saved. The bot follows it from the next command or message."));
      },
    });
  };

  return (
    <>
      {data.groups.map((group) => {
        const items = data.items.filter((m) => m.group === group.key);
        // A group no module belongs to yet stays off the page.
        if (items.length === 0) return null;
        return (
          <Panel
            key={group.key}
            title={spaTranslate(group.label).toLocaleUpperCase(locale() === "tr" ? "tr-TR" : "en-US")}
          >
            <div className="settings-detail-list">
              {items.map((saved) => (
                <ModuleRow
                  key={saved.key}
                  saved={saved}
                  draft={drafts[saved.key]}
                  info={discord.data}
                  max={data.max_channels}
                  error={refused?.key === saved.key ? refused.message : null}
                  onChange={(next) => edit(saved.key, next)}
                />
              ))}
            </div>
          </Panel>
        );
      })}
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={blocked}
        error={save.isError && !refused ? save.error.message : null}
        onSave={() => submit()}
        onReset={() => {
          setDrafts({});
          save.reset();
        }}
      />
      <Toast message={toast} onDone={clearToast} />
      <ModuleOffConfirm
        open={confirmOff}
        onCancel={() => setConfirmOff(false)}
        onConfirm={() => {
          setConfirmOff(false);
          submit(true);
        }}
      />
    </>
  );
}

function ModuleRow({
  saved,
  draft,
  info,
  max,
  error,
  onChange,
}: {
  saved: ModuleEntry;
  draft: ModuleDraft | undefined;
  info: GuildInfo | undefined;
  max: number;
  error: string | null;
  onChange: (next: ModuleDraft) => void;
}) {
  const now = effective(saved, draft);
  const why = problem(saved, draft, max);
  const edited = patchFor(saved, draft) !== null;
  return (
    <div className={`module-settings-row ${edited ? "is-edited" : ""}`} data-module={saved.key}>
      <SettingsRow
        rowId={`module-${saved.key}`}
        title={spaTranslate(saved.label)}
        description={spaTranslate(saved.blurb)}
        summary={moduleSummary(saved, draft, info)}
        enabled={now.enabled}
        onToggle={(enabled) => onChange({ ...draft, enabled })}
        toggleLabel={spaTranslate(saved.label)}
      >
        <div className="settings-detail-fields">
          {saved.server_only ? (
            <p className="module-scope muted">{deferredText("Whole server")}</p>
          ) : (
            <>
              <Choice
                name={deferredText("Where {arg1} works", { arg1: saved.label })}
                value={now.mode}
                options={MODES}
                onChange={(mode) => onChange(withMode(saved, draft, mode))}
              />
              {now.mode !== "all" && (
                <ModuleChannels
                  info={info}
                  max={max}
                  channels={now.channels}
                  onChange={(channels) => onChange(withChannels(withMode(saved, draft, now.mode), channels))}
                />
              )}
            </>
          )}
        </div>
      </SettingsRow>
      {(error || why) && (
        <p className="field-error" role="alert">
          {error ?? why}
        </p>
      )}
    </div>
  );
}
