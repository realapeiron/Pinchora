import { useCallback, useId, useState } from "react";
import { useOutletContext } from "react-router";

import { Choice } from "../components/Choice";
import { Panel } from "../components/Panel";
import { ChannelPicker, ChannelTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { ChannelMode, GuildInfo, ModuleEntry, Modules as ModulesData, Overview } from "../lib/api";
import {
  type ModuleDraft,
  badge,
  changeCount,
  effective,
  patchFor,
  problem,
  withChannels,
  withMode,
} from "../lib/modules";
import { ModuleSaveError, useDiscord, useModules, useSaveModules } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

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
    <div className="settings-page">
      <PageHead
        title="MODULES"
        lead="Switch parts of Pinchora off for the whole server, or keep them to some channels. Switched-off commands answer privately that the feature is off."
      />
      {query.isPending && <Loading label="Reading the modules" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <ModulesForm guild={guild} data={query.data} />}
    </div>
  );
}

function ModulesForm({ guild, data }: { guild: string; data: ModulesData }) {
  const discord = useDiscord(guild);
  const save = useSaveModules(guild);
  const [drafts, setDrafts] = useState<Record<string, ModuleDraft>>({});
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);

  const changes = changeCount(data.items, drafts);
  const blocked = data.items.some((m) => problem(m, drafts[m.key], data.max_channels) !== null);
  const refused = save.error instanceof ModuleSaveError ? save.error : null;
  const edit = (key: string, next: ModuleDraft) => {
    save.reset();
    setDrafts((all) => ({ ...all, [key]: next }));
  };

  const submit = () => {
    const todo = data.items.flatMap((saved) => {
      const patch = patchFor(saved, drafts[saved.key]);
      return patch ? [{ key: saved.key, patch }] : [];
    });
    save.mutate(todo, {
      onSuccess: () => {
        setDrafts({});
        setToast("Saved. The bot follows it from the next command or message.");
      },
    });
  };

  return (
    <>
      {data.groups.map((group) => (
        <Panel key={group.key} title={group.label.toUpperCase()}>
          <ul className="module-list">
            {data.items
              .filter((m) => m.group === group.key)
              .map((saved) => (
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
          </ul>
        </Panel>
      ))}
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={blocked}
        error={save.isError && !refused ? save.error.message : null}
        onSave={submit}
        onReset={() => {
          setDrafts({});
          save.reset();
        }}
      />
      <Toast message={toast} onDone={clearToast} />
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
  const [open, setOpen] = useState(false);
  const sheet = useId();
  const now = effective(saved, draft);
  const state = badge(now.enabled, now.mode, now.channels.length);
  const why = problem(saved, draft, max);
  const edited = patchFor(saved, draft) !== null;
  return (
    <li className={`module-row ${edited ? "is-edited" : ""}`} data-module={saved.key}>
      <div className="module-main">
        <div className="module-title">
          <h3 className="setting-name">{saved.label}</h3>
          <span className={`badge badge-${state.tone}`}>{state.text}</span>
        </div>
        <p className="muted">{saved.blurb}</p>
        {(error || why) && (
          <p className="field-error" role="alert">
            {error ?? why}
          </p>
        )}
      </div>
      <div className="module-actions">
        {saved.server_only ? (
          <span className="module-scope muted">Whole server</span>
        ) : (
          <PixelButton
            variant="ghost"
            className="button-small"
            aria-expanded={open}
            aria-controls={sheet}
            onClick={() => setOpen(!open)}
          >
            {open ? "Close" : "Channels"}
          </PixelButton>
        )}
        <PixelToggle
          label={saved.label}
          checked={now.enabled}
          onChange={(enabled) => onChange({ ...draft, enabled })}
        />
      </div>
      {open && !saved.server_only && (
        <div className="module-sheet" id={sheet}>
          <Choice
            name={`Where ${saved.label} works`}
            value={now.mode}
            options={MODES}
            onChange={(mode) => onChange(withMode(saved, draft, mode))}
          />
          {now.mode !== "all" && (
            <ChannelList
              info={info}
              max={max}
              channels={now.channels}
              onChange={(channels) => onChange(withChannels(withMode(saved, draft, now.mode), channels))}
            />
          )}
        </div>
      )}
    </li>
  );
}

function ChannelList({
  info,
  max,
  channels,
  onChange,
}: {
  info: GuildInfo | undefined;
  max: number;
  channels: string[];
  onChange: (next: string[]) => void;
}) {
  const [pick, setPick] = useState<string | null>(null);
  const full = channels.length >= max;
  return (
    <div className="module-channels">
      {channels.length === 0 ? (
        <p className="muted">No channels yet.</p>
      ) : (
        <ul className="channel-chips" aria-label="Channels">
          {channels.map((id) => (
            <li key={id} className="channel-chip">
              <ChannelTag info={info} id={id} />
              <button
                type="button"
                className="chip-remove"
                aria-label={`Remove ${info?.channels.find((c) => c.id === id)?.name ?? id}`}
                onClick={() => onChange(channels.filter((c) => c !== id))}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="module-add">
        <ChannelPicker
          label="Add a channel"
          info={info}
          value={pick}
          onChange={setPick}
          any
          none={full ? `Up to ${max} channels` : "Pick a channel or category"}
          disabled={full}
        />
        <PixelButton
          variant="secondary"
          className="button-small"
          disabled={!pick || full || channels.includes(pick)}
          onClick={() => {
            if (pick) onChange([...channels, pick]);
            setPick(null);
          }}
        >
          Add
        </PixelButton>
      </div>
      <p className="field-hint muted">
        A category covers the channels and threads under it. {channels.length} of {max}.
      </p>
    </div>
  );
}
