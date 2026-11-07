import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import { ApiError, type LadderAction, type ModerationSettings, type Overview } from "../lib/api";
import { type DraftRung, draftLadder, moderationPatch } from "../lib/moderation";
import { useDiscord, useModeration } from "../lib/queries";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";

const ACTIONS: { key: LadderAction; label: string }[] = [
  { key: "timeout", label: "Timeout" },
  { key: "kick", label: "Kick" },
  { key: "ban", label: "Ban" },
];

/** What the length box says while empty. */
function placeholder(action: LadderAction): string {
  if (action === "kick") return "no length";
  return action === "ban" ? "for good" : "1h";
}

export function Moderation() {
  const data = useOutletContext<Overview>();
  const { query } = useModeration(data.guild.id);
  return (
    <div className="settings-page mod-page">
      <PageHead
        title="MODERATION"
        lead="Where the cases of /ban, /kick, /timeout, /warn and the others are posted, whether members hear about them, and what enough warns bring."
      />
      {query.isPending && <Loading label="Reading the settings" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <ModerationForm guild={data.guild.id} saved={query.data} />}
    </div>
  );
}

function ModerationForm({ guild, saved }: { guild: string; saved: ModerationSettings }) {
  const discord = useDiscord(guild);
  const { save } = useModeration(guild);
  const [channel, setChannel] = useState(saved.log_channel_id);
  const [dm, setDm] = useState(saved.dm_members);
  const [ladder, setLadder] = useState<DraftRung[]>(() => draftLadder(saved));
  const [adding, setAdding] = useState<DraftRung>({ warns: 3, action: "timeout", duration: "1h" });
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const { patch, count } = moderationPatch(saved, channel, dm, ladder);
  const error = save.error instanceof ApiError ? save.error : null;
  const fieldError = (field: string) => (error?.field === field ? error.message : null);
  const full = ladder.length >= saved.max_rungs;
  const taken = ladder.some((r) => r.warns === adding.warns);

  const change = (n: number, next: Partial<DraftRung>) =>
    setLadder((rungs) => rungs.map((r, i) => (i === n ? { ...r, ...next } : r)));
  const add = () => {
    setLadder((rungs) => [...rungs, adding].sort((a, b) => a.warns - b.warns));
    setAdding({ ...adding, warns: adding.warns + 2 });
  };
  const submit = () =>
    save.mutate(patch, {
      onSuccess: (now) => {
        setLadder(draftLadder(now));
        setToast("Saved. The next case follows it.");
      },
    });
  const reset = () => {
    setChannel(saved.log_channel_id);
    setDm(saved.dm_members);
    setLadder(draftLadder(saved));
    save.reset();
  };

  return (
    <>
      <Panel title="MOD-LOG">
        <Field
          label="Mod-log channel"
          htmlFor="modlog-channel"
          error={fieldError("log_channel_id")}
          hint="Every case is posted here with its number, the moderator, the member, the reason and how long. Keep it private to staff."
        >
          <ChannelPicker
            id="modlog-channel"
            info={discord.data}
            value={channel}
            onChange={setChannel}
            none="The server log's moderation channel"
          />
        </Field>
        <SwitchRow
          title="Tell members"
          control={<PixelToggle label="Tell members" checked={dm} onChange={setDm} />}
        >
          A direct message when they are warned, timed out, kicked or banned, with the reason and how long.
          Members with closed direct messages are skipped; the case says so.
        </SwitchRow>
      </Panel>

      <Panel title="WARN LADDER">
        <p className="panel-lead muted">
          When a warn brings a member's active warns to a number on the ladder, Pinchora acts on its own, as a
          case of its own. Taking a warn back with /mod unwarn lowers the count again.
        </p>
        {ladder.length === 0 ? (
          <p className="empty-row muted">No rungs yet: warns are only counted.</p>
        ) : (
          <ul className="list" aria-label="Warn ladder">
            {ladder.map((rung, n) => (
              <li key={`${rung.warns}-${n}`} className="list-row ladder-row">
                <span className="list-level pixel-label">{rung.warns} warns</span>
                <select
                  aria-label={`What ${rung.warns} warns bring`}
                  className="pixel-input pixel-select"
                  value={rung.action}
                  onChange={(event) => change(n, { action: event.target.value as LadderAction })}
                >
                  {ACTIONS.map((a) => (
                    <option key={a.key} value={a.key}>
                      {a.label}
                    </option>
                  ))}
                </select>
                <input
                  aria-label={`How long, at ${rung.warns} warns`}
                  className="pixel-input input-short"
                  value={rung.duration}
                  disabled={rung.action === "kick"}
                  placeholder={placeholder(rung.action)}
                  onChange={(event) => change(n, { duration: event.target.value })}
                />
                <button
                  type="button"
                  className="chip-remove"
                  aria-label={`Remove the rung at ${rung.warns} warns`}
                  onClick={() => setLadder((rungs) => rungs.filter((_, i) => i !== n))}
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
        {fieldError("ladder") && (
          <p className="field-error" role="alert">
            {fieldError("ladder")}
          </p>
        )}
        <div className="add-row add-row-role">
          <Field label="Warns" htmlFor="rung-warns">
            <input
              id="rung-warns"
              type="number"
              min={1}
              max={50}
              className="pixel-input"
              value={adding.warns}
              onChange={(event) => setAdding({ ...adding, warns: Number(event.target.value) })}
            />
          </Field>
          <Field label="Brings" htmlFor="rung-action">
            <div className="ladder-add">
              <select
                id="rung-action"
                className="pixel-input pixel-select"
                value={adding.action}
                onChange={(event) =>
                  setAdding({
                    ...adding,
                    action: event.target.value as LadderAction,
                    duration: event.target.value === "kick" ? "" : adding.duration,
                  })
                }
              >
                {ACTIONS.map((a) => (
                  <option key={a.key} value={a.key}>
                    {a.label}
                  </option>
                ))}
              </select>
              <input
                aria-label="How long"
                className="pixel-input input-short"
                value={adding.duration}
                disabled={adding.action === "kick"}
                placeholder={placeholder(adding.action)}
                onChange={(event) => setAdding({ ...adding, duration: event.target.value })}
              />
            </div>
          </Field>
          <div className="add-actions">
            <PixelButton
              variant="secondary"
              disabled={full || taken || adding.warns < 1 || adding.warns > 50}
              onClick={add}
            >
              Add rung
            </PixelButton>
          </div>
        </div>
        {full && (
          <p className="notice">The ladder holds {saved.max_rungs} rungs. Remove one to add another.</p>
        )}
      </Panel>

      <SaveBar
        count={count}
        saving={save.isPending}
        error={
          save.isError && !fieldError("ladder") && !fieldError("log_channel_id") ? save.error.message : null
        }
        onSave={submit}
        onReset={reset}
      />
      <Toast message={toast} onDone={clearToast} />
    </>
  );
}
