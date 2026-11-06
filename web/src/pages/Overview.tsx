import { useCallback, useState } from "react";
import { useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { Overview as OverviewData, Settings } from "../lib/api";
import { changedFields, patchOf } from "../lib/draft";
import { useSaveSettings } from "../lib/queries";
import { describeChange, timeAgo } from "../lib/text";

export function Overview() {
  const data = useOutletContext<OverviewData>();
  const guild = data.guild.id;
  const [draft, setDraft] = useState<Partial<Settings>>({});
  const [toast, setToast] = useState<string | null>(null);
  const save = useSaveSettings(guild);
  const saved = data.settings;
  const changes = changedFields(saved, draft).length;
  const leveling = (draft.leveling_enabled ?? saved.leveling_enabled) as boolean;
  const clearToast = useCallback(() => setToast(null), []);

  const onSave = () =>
    save.mutate(patchOf(saved, draft), {
      onSuccess: () => {
        setDraft({});
        setToast("Saved. The bot uses it from the next message.");
      },
    });

  return (
    <div className="overview">
      <div className="page-head">
        <div>
          <h1 className="pixel-heading">OVERVIEW</h1>
          <p className="muted">{data.guild.name}</p>
        </div>
      </div>

      <Panel
        title="LEVELING"
        aside={
          <span
            className={`badge ${saved.leveling_enabled ? "badge-mint" : "badge-dim"}`}
            title="What the bot uses right now"
          >
            {saved.leveling_enabled ? "LIVE" : "PAUSED"}
          </span>
        }
      >
        <div className="setting-row">
          <div className="setting-text">
            <h3 className="setting-name" id="chat-xp">
              Chat XP
            </h3>
            <p className="muted">
              Members earn XP for chatting, climb levels and get a level-up card. Turn it off to pause XP in
              this server; levels already earned stay.
            </p>
          </div>
          <PixelToggle
            label="Chat XP"
            checked={leveling}
            onChange={(next) => setDraft((d) => ({ ...d, leveling_enabled: next }))}
            disabled={save.isPending}
          />
        </div>
      </Panel>

      <Panel title="RECENT CHANGES">
        {data.audit.length === 0 ? (
          <p className="muted">
            No changes from the dashboard yet. What you save here shows up in this list.
          </p>
        ) : (
          <ol className="audit-list">
            {data.audit.map((entry) => (
              <li key={entry.id} className="audit-row">
                <span className="audit-dot" aria-hidden="true" />
                <span className="audit-text">
                  <strong>{entry.actor_name}</strong> {describeChange(entry)}
                  {entry.source === "discord" && <span className="muted"> in Discord</span>}
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
        count={changes}
        saving={save.isPending}
        error={save.isError ? save.error.message : null}
        onSave={onSave}
        onReset={() => {
          setDraft({});
          save.reset();
        }}
      />
      <Toast message={toast} onDone={clearToast} />
    </div>
  );
}
