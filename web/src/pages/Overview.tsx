import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { Overview as OverviewData, Settings } from "../lib/api";
import { changedFields, patchOf } from "../lib/draft";
import { badge } from "../lib/modules";
import { useNames, useSaveSettings } from "../lib/queries";
import { describeChange, timeAgo } from "../lib/text";

const MODE_WORDS: Record<string, string> = {
  channel: "level-ups in their channel",
  current: "level-ups where they chat",
  dm: "level-ups by DM",
  off: "no level-up messages",
};

/** One tile per settings page: is its main switch on, and the gist of how it is set. */
function glance(s: Settings) {
  return [
    {
      to: "welcome",
      name: "Welcome cards",
      on: s.welcome_enabled && s.welcome_channel_id !== null,
      detail:
        (s.welcome_channel_id ? `${s.welcome_theme ?? "reef"} theme` : "no channel yet") +
        (s.join_role_id ? ", join role" : ""),
    },
    {
      to: "leveling",
      name: "Leveling",
      on: s.leveling_enabled,
      detail: `${MODE_WORDS[s.levelup_mode] ?? s.levelup_mode}${s.voice_xp_enabled ? ", voice XP" : ""}`,
    },
    {
      to: "points",
      name: "Server points",
      on: s.points_enabled,
      detail: `${s.points_emoji} ${s.points_min}–${s.points_max} ${s.points_name} a message`,
    },
    {
      to: "welcome",
      name: "Goodbye",
      on: s.goodbye_channel_id !== null,
      detail: s.goodbye_channel_id ? "when someone leaves" : "no channel yet",
    },
    {
      to: "log",
      name: "Server log",
      on: s.log_channel_id !== null,
      detail:
        [s.log_members && "joins", s.log_names && "names", s.log_bans && "bans", s.log_voice && "voice"]
          .filter(Boolean)
          .join(", ") || "nothing picked",
    },
  ];
}

export function Overview() {
  const data = useOutletContext<OverviewData>();
  const guild = data.guild.id;
  const [draft, setDraft] = useState<Partial<Settings>>({});
  const [toast, setToast] = useState<string | null>(null);
  const save = useSaveSettings(guild);
  const names = useNames(guild);
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

      <Panel title="AT A GLANCE">
        <ul className="glance">
          {glance(saved).map((tile) => (
            <li key={tile.name}>
              <Link to={`/g/${guild}/${tile.to}`} className="glance-tile">
                <span className="glance-name">{tile.name}</span>
                <span className={`badge ${tile.on ? "badge-mint" : "badge-dim"}`}>
                  {tile.on ? "ON" : "OFF"}
                </span>
                <span className="glance-detail muted">{tile.detail}</span>
              </Link>
            </li>
          ))}
        </ul>
      </Panel>

      <Panel
        title="MODULES"
        aside={
          <PixelLink variant="ghost" className="button-small" href={`/g/${guild}/modules`}>
            Manage
          </PixelLink>
        }
      >
        <ul className="module-grid" aria-label="Modules">
          {data.modules.map((m) => {
            const state = badge(m.enabled, m.channel_mode, m.channels.length);
            return (
              <li key={m.key}>
                <Link to={`/g/${guild}/modules`} className="module-tile" title={m.blurb}>
                  <span className="module-tile-name">{m.label}</span>
                  <span className={`badge badge-${state.tone}`}>{state.text}</span>
                </Link>
              </li>
            );
          })}
        </ul>
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
                  <strong>{entry.actor_name}</strong> {describeChange(entry, names)}
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
