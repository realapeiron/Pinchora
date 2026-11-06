import { type ReactNode, useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, ChannelTag, RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type GuildInfo, type LogSettings, type Overview } from "../lib/api";
import { isDiscordId } from "../lib/ids";
import { type LogDraft, logDraft, logsPatch } from "../lib/moderation";
import { useDiscord, useLogs } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

export function ServerLog() {
  const data = useOutletContext<Overview>();
  const { query } = useLogs(data.guild.id);
  return (
    <div className="settings-page mod-page">
      <PageHead
        title="SERVER LOG"
        lead="A channel (or one per category) where the bot notes who came and went, what was edited or deleted, and what staff changed."
      />
      {query.isPending && <Loading label="Reading the log settings" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <LogForm guild={data.guild.id} saved={query.data} />}
    </div>
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
        setToast("Saved. The log follows it from the next event.");
      },
    });
  const reset = () => {
    setDraft(logDraft(saved));
    save.reset();
  };

  return (
    <>
      <Panel title="LOG CHANNEL">
        <Field
          label="Channel"
          htmlFor="log-channel"
          error={fieldError("channel_id")}
          hint="Picking a channel turns the log on. Keep it private to moderators. A category below can have a channel of its own."
        >
          <ChannelPicker
            id="log-channel"
            info={discord.data}
            value={draft.channel_id}
            onChange={(id) => set({ channel_id: id })}
            none="Off: no server log"
            invalid={!!fieldError("channel_id")}
          />
        </Field>
      </Panel>

      <Panel title="WHAT IT REPORTS">
        <p className="panel-lead muted">
          Entries marked <span className="badge badge-dim">audit log</span> come from Discord's audit log and
          say who did it; Pinchora needs the View Audit Log permission for them.
        </p>
        {fieldError("category_channels") && (
          <p className="field-error" role="alert">
            {fieldError("category_channels")}
          </p>
        )}
        <div className={`log-matrix ${on ? "" : "is-muted"}`}>
          {saved.categories.map((category) => (
            <section key={category.key} className="log-category" aria-label={category.label}>
              <header className="log-category-head">
                <h3 className="setting-name">{category.label}</h3>
                <ChannelPicker
                  label={`Channel for ${category.label}`}
                  info={discord.data}
                  value={draft.category_channels[category.key] ?? null}
                  onChange={(id) =>
                    set({ category_channels: { ...draft.category_channels, [category.key]: id } })
                  }
                  none="The log channel"
                />
              </header>
              <ul className="log-events">
                {category.events.map((event) => (
                  <li key={event.key} className="log-event">
                    <span className="log-event-name">
                      {event.label}
                      {event.audit_log && <span className="badge badge-dim">audit log</span>}
                    </span>
                    <PixelToggle
                      label={event.label}
                      checked={draft.events[event.key] ?? false}
                      onChange={(next) => set({ events: { ...draft.events, [event.key]: next } })}
                    />
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        {!on && <p className="notice">Pick a log channel above; until then nothing is reported.</p>}
      </Panel>

      <Panel title="LEFT ALONE">
        <p className="panel-lead muted">
          Nothing is logged about these members, members with these roles, or what happens in these channels
          (a category covers its channels, a channel its threads). Up to {saved.max_ignored} of each.
        </p>
        <SwitchRow
          title="Bots"
          control={
            <PixelToggle
              label="Leave bots alone"
              checked={draft.ignore_bots}
              onChange={(bots) => set({ ignore_bots: bots })}
            />
          }
        >
          Messages, joins and changes of bots stay out of the log.
        </SwitchRow>
        <IgnoreChannels
          info={discord.data}
          list={draft.ignored_channels}
          onChange={(list) => set({ ignored_channels: list })}
          error={fieldError("ignored_channels")}
        />
        <IgnoreRoles
          info={discord.data}
          list={draft.ignored_roles}
          onChange={(list) => set({ ignored_roles: list })}
          error={fieldError("ignored_roles")}
        />
        <IgnoreUsers
          list={draft.ignored_users}
          onChange={(list) => set({ ignored_users: list })}
          error={fieldError("ignored_users")}
        />
      </Panel>

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

function Chips({ children, empty }: { children: ReactNode[]; empty: string }) {
  if (children.length === 0) return <p className="muted ignore-empty">{empty}</p>;
  return <ul className="chips ignore-chips">{children}</ul>;
}

function RemoveChip({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button type="button" className="chip-remove" aria-label={label} onClick={onClick}>
      ×
    </button>
  );
}

function IgnoreChannels({
  info,
  list,
  onChange,
  error,
}: {
  info: GuildInfo | undefined;
  list: string[];
  onChange: (list: string[]) => void;
  error: string | null;
}) {
  const [pick, setPick] = useState<string | null>(null);
  return (
    <Field label="Channels and categories" htmlFor="ignore-channel" error={error}>
      <div className="ignore-add">
        <ChannelPicker
          id="ignore-channel"
          info={info}
          value={pick}
          onChange={setPick}
          none="Pick a channel"
          any
        />
        <PixelButton
          variant="secondary"
          disabled={!pick || list.includes(pick)}
          onClick={() => {
            if (pick) onChange([...list, pick]);
            setPick(null);
          }}
        >
          Add
        </PixelButton>
      </div>
      <Chips empty="No channels.">
        {list.map((id) => (
          <li key={id} className="channel-chip">
            <ChannelTag info={info} id={id} />
            <RemoveChip
              label="Log this channel again"
              onClick={() => onChange(list.filter((x) => x !== id))}
            />
          </li>
        ))}
      </Chips>
    </Field>
  );
}

function IgnoreRoles({
  info,
  list,
  onChange,
  error,
}: {
  info: GuildInfo | undefined;
  list: string[];
  onChange: (list: string[]) => void;
  error: string | null;
}) {
  const [pick, setPick] = useState<string | null>(null);
  return (
    <Field label="Roles" htmlFor="ignore-role" error={error}>
      <div className="ignore-add">
        <RolePicker id="ignore-role" info={info} value={pick} onChange={setPick} none="Pick a role" />
        <PixelButton
          variant="secondary"
          disabled={!pick || list.includes(pick)}
          onClick={() => {
            if (pick) onChange([...list, pick]);
            setPick(null);
          }}
        >
          Add
        </PixelButton>
      </div>
      <Chips empty="No roles.">
        {list.map((id) => (
          <li key={id} className="channel-chip">
            <RoleTag info={info} id={id} />
            <RemoveChip label="Log this role again" onClick={() => onChange(list.filter((x) => x !== id))} />
          </li>
        ))}
      </Chips>
    </Field>
  );
}

function IgnoreUsers({
  list,
  onChange,
  error,
}: {
  list: string[];
  onChange: (list: string[]) => void;
  error: string | null;
}) {
  const [typed, setTyped] = useState("");
  const id = typed.trim();
  const valid = isDiscordId(id);
  return (
    <Field
      label="Members"
      htmlFor="ignore-user"
      error={error}
      hint="Paste a member's user id (with Developer Mode on in Discord: right-click them, Copy User ID)."
    >
      <div className="ignore-add">
        <input
          id="ignore-user"
          className="pixel-input"
          inputMode="numeric"
          placeholder="User id"
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          aria-invalid={(id !== "" && !valid) || undefined}
        />
        <PixelButton
          variant="secondary"
          disabled={!valid || list.includes(id)}
          onClick={() => {
            onChange([...list, id]);
            setTyped("");
          }}
        >
          Add
        </PixelButton>
      </div>
      <Chips empty="No members.">
        {list.map((user) => (
          <li key={user} className="channel-chip">
            <span className="channel-tag">{user}</span>
            <RemoveChip
              label="Log this member again"
              onClick={() => onChange(list.filter((x) => x !== user))}
            />
          </li>
        ))}
      </Chips>
    </Field>
  );
}
