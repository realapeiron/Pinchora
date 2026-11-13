import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { ChannelPicker } from "../../../components/DashboardPickers";
import { PixelButton } from "../../../components/PixelButton";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import { ApiError, type LadderAction, type ModerationSettings } from "../../../lib/api";
import { type DraftRung, draftLadder, moderationPatch } from "../api/moderation";
import { useDiscord, useModeration } from "../../../lib/queries";
import { t } from "../../../i18n/dashboard";
import { moderationSummary } from "./moderationSummary";

const ACTIONS: { key: LadderAction; label: string }[] = [
  { key: "timeout", label: "Timeout" },
  { key: "kick", label: "Kick" },
  { key: "ban", label: "Ban" },
];

/** What the length box says while empty. */
function placeholder(action: LadderAction): string {
  if (action === "kick") return deferredText("no length");
  return action === "ban" ? spaTranslate("for good") : "1h";
}

export function ModerationForm({ guild, saved }: { guild: string; saved: ModerationSettings }) {
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
  const summary = moderationSummary(channel, dm, ladder.length, discord.data);

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
        setToast(deferredText("Saved. The next case follows it."));
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
      <div className="settings-detail-list">
        <SettingsRow
          rowId="moderation-form-mod-log"
          title={t("Mod log", {}, "MOD-LOG")}
          description={t("Choose where moderation cases are posted.")}
          summary={summary.log}
          initialOpen
        >
          <div className="settings-detail-fields">
            <Field
              label={deferredText("Mod-log channel")}
              htmlFor="modlog-channel"
              error={fieldError("log_channel_id")}
              hint={deferredText(
                "Every case is posted here with its number, the moderator, the member, the reason and how long. Keep it private to staff.",
              )}
            >
              <ChannelPicker
                posting
                id="modlog-channel"
                info={discord.data}
                value={channel}
                onChange={setChannel}
                none={deferredText("The server log's moderation channel")}
              />
            </Field>
          </div>
        </SettingsRow>
        <SettingsRow
          rowId="moderation-form-tell-members"
          title={deferredText("Tell members")}
          description={t("Send members a message about their cases.")}
          summary={summary.dm}
          enabled={dm}
          onToggle={setDm}
        >
          <p className="muted">
            {deferredText(
              "A direct message when they are warned, timed out, kicked or banned, with the reason and how long. Members with closed direct messages are skipped; the case says so.",
            )}
          </p>
        </SettingsRow>

        <SettingsRow
          rowId="moderation-form-warn-ladder"
          title={t("row.Warn ladder", {}, "WARN LADDER")}
          description={t("Act when a member reaches a warn count.")}
          summary={summary.ladder}
        >
          <div className="settings-detail-fields">
            <p className="panel-lead muted">
              {" "}
              {deferredText(
                "When a warn brings a member's active warns to a number on the ladder, Pinchora acts on its own, as a case of its own. Taking a warn back with /mod unwarn lowers the count again.",
              )}{" "}
            </p>
            {ladder.length === 0 ? (
              <p className="empty-row muted">{deferredText("No rungs yet: warns are only counted.")}</p>
            ) : (
              <ul className="list" aria-label={spaTranslate("Warn ladder")}>
                {ladder.map((rung, n) => (
                  <li key={`${rung.warns}-${n}`} className="list-row ladder-row">
                    <span className="list-level pixel-label">
                      {rung.warns} {spaTranslate("warns")}
                    </span>
                    <select
                      aria-label={deferredText("What {arg1} warns bring", { arg1: rung.warns })}
                      className="pixel-input pixel-select"
                      value={rung.action}
                      onChange={(event) => change(n, { action: event.target.value as LadderAction })}
                    >
                      {ACTIONS.map((a) => (
                        <option key={a.key} value={a.key}>
                          {spaTranslate(a.label)}
                        </option>
                      ))}
                    </select>
                    <input
                      aria-label={deferredText("How long, at {arg1} warns", { arg1: rung.warns })}
                      className="pixel-input input-short"
                      value={rung.duration}
                      disabled={rung.action === "kick"}
                      placeholder={placeholder(rung.action)}
                      onChange={(event) => change(n, { duration: event.target.value })}
                    />
                    <button
                      type="button"
                      className="chip-remove"
                      aria-label={deferredText("Remove the rung at {arg1} warns", { arg1: rung.warns })}
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
              <Field label={deferredText("Warns")} htmlFor="rung-warns">
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
              <Field label={deferredText("Brings")} htmlFor="rung-action">
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
                        {spaTranslate(a.label)}
                      </option>
                    ))}
                  </select>
                  <input
                    aria-label={spaTranslate("How long")}
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
                  {" "}
                  {deferredText("Add rung")}{" "}
                </PixelButton>
              </div>
            </div>
            {full && (
              <p className="notice">
                {deferredText("The ladder holds")} {saved.max_rungs}{" "}
                {deferredText("rungs. Remove one to add another.")}
              </p>
            )}
          </div>
        </SettingsRow>
      </div>

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
