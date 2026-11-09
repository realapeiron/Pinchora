import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Field } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { PixelToggle } from "../../../components/PixelToggle";
import { SaveBar } from "../../../components/SaveBar";
import { Toast } from "../../../components/Toast";
import type { ModuleEntry, Overview as OverviewData, Settings } from "../../../lib/api";
import { changedFields, patchOf } from "../../../lib/draft";
import { badge } from "../api/modules";
import { useNames, useSaveSettings } from "../../../lib/queries";
import { DEFAULTS, LIMITS, checkPrefix } from "../../../lib/rules";
import { describeChange, timeAgo } from "../../../lib/text";

const MODE_WORDS: Record<string, string> = {
  channel: "level-ups in their channel",
  current: "level-ups where they chat",
  dm: "level-ups by DM",
  off: "no level-up messages",
};

/**
 * One tile per settings page: is it on, and the gist of how it is set. "On" is the module's own switch (the
 * grid below shows the same), and a page that also needs a channel is on only once it has one.
 */
function glance(s: Settings, modules: ModuleEntry[]) {
  const switched = (key: string) => modules.find((m) => m.key === key)?.enabled ?? true;
  const off = spaTranslate("switched off");
  return [
    {
      to: "welcome",
      name: spaTranslate("Welcome cards"),
      on: switched("welcome") && s.welcome_channel_id !== null,
      detail: !switched("welcome")
        ? off
        : (s.welcome_channel_id
            ? spaTranslate("{theme} theme", { theme: s.welcome_theme ?? "reef" })
            : spaTranslate("no channel yet")) + (s.join_role_id ? spaTranslate(", join role") : ""),
    },
    {
      to: "leveling",
      name: spaTranslate("Leveling"),
      on: switched("leveling"),
      detail: `${spaTranslate(MODE_WORDS[s.levelup_mode] ?? s.levelup_mode)}${s.voice_xp_enabled ? spaTranslate(", voice XP") : ""}`,
    },
    {
      to: "points",
      name: spaTranslate("Server points"),
      on: switched("points"),
      detail: spaTranslate("{emoji} {min}–{max} {name} per message", {
        emoji: s.points_emoji,
        min: s.points_min,
        max: s.points_max,
        name: s.points_name,
      }),
    },
    {
      to: "welcome",
      name: spaTranslate("Goodbye"),
      on: switched("goodbye") && s.goodbye_channel_id !== null,
      detail: !switched("goodbye")
        ? off
        : s.goodbye_channel_id
          ? spaTranslate("when someone leaves")
          : spaTranslate("no channel yet"),
    },
    {
      to: "log",
      name: spaTranslate("Server log"),
      on: switched("log") && s.log_channel_id !== null,
      detail: !switched("log")
        ? off
        : s.log_channel_id === null
          ? spaTranslate("no channel yet")
          : [s.log_members && "joins", s.log_names && "names", s.log_bans && "bans", s.log_voice && "voice"]
              .filter(Boolean)
              .map((word) => spaTranslate(String(word)))
              .join(", ") || spaTranslate("nothing picked"),
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
  const prefix = draft.prefix ?? saved.prefix;
  const prefixCheck = checkPrefix(prefix);
  const prefixError = draft.prefix !== undefined && !prefixCheck.ok ? prefixCheck.message : null;
  const clearToast = useCallback(() => setToast(null), []);

  const onSave = () => {
    if (prefixError) return;
    save.mutate(patchOf(saved, draft), {
      onSuccess: () => {
        setDraft({});
        setToast(spaTranslate("Saved. The bot uses it from the next message."));
      },
    });
  };

  return (
    <div className="overview">
      <div className="page-head">
        <div>
          <h1 className="pixel-heading">{spaTranslate("OVERVIEW")}</h1>
          <p className="muted">{data.guild.name}</p>
        </div>
      </div>

      <Panel
        title={spaTranslate("LEVELING")}
        aside={
          <span
            className={`badge ${saved.leveling_enabled ? "badge-mint" : "badge-dim"}`}
            title={spaTranslate("What the bot uses right now")}
          >
            {saved.leveling_enabled ? "LIVE" : "PAUSED"}
          </span>
        }
      >
        <div className="setting-row">
          <div className="setting-text">
            <h3 className="setting-name" id="chat-xp">
              {" "}
              {spaTranslate("Chat XP")}{" "}
            </h3>
            <p className="muted">
              {" "}
              {spaTranslate(
                "Members earn XP for chatting, climb levels and get a level-up card. Turn it off to pause XP in this server; levels already earned stay.",
              )}{" "}
            </p>
          </div>
          <PixelToggle
            label={spaTranslate("Chat XP")}
            checked={leveling}
            onChange={(next) => setDraft((d) => ({ ...d, leveling_enabled: next }))}
            disabled={save.isPending}
          />
        </div>
      </Panel>

      <Panel title={spaTranslate("TEXT COMMANDS")}>
        <Field
          label={spaTranslate("Prefix")}
          htmlFor="text-prefix"
          error={prefixError}
          hint={spaTranslate("Every command also works as text: {arg1}profile, or @Pinchora profile.", {
            arg1: prefix.trim() || DEFAULTS.prefix,
          })}
          aside={
            prefix !== DEFAULTS.prefix && (
              <PixelButton
                variant="ghost"
                className="button-small"
                onClick={() => setDraft((d) => ({ ...d, prefix: DEFAULTS.prefix }))}
                disabled={save.isPending}
              >
                {" "}
                {spaTranslate("Default")}{" "}
              </PixelButton>
            )
          }
        >
          <input
            id="text-prefix"
            className="pixel-input input-short"
            value={prefix}
            maxLength={LIMITS.prefix_max + 8}
            onChange={(event) => setDraft((d) => ({ ...d, prefix: event.target.value }))}
            aria-invalid={prefixError ? true : undefined}
            disabled={save.isPending}
          />
        </Field>
      </Panel>

      <Panel title={spaTranslate("AT A GLANCE")}>
        <ul className="glance">
          {glance(saved, data.modules).map((tile) => (
            <li key={tile.name}>
              <Link to={`/g/${guild}/${tile.to}`} className="glance-tile">
                <span className="glance-name">{spaTranslate(tile.name)}</span>
                <span className={`badge ${tile.on ? "badge-mint" : "badge-dim"}`}>
                  {tile.on ? "ON" : "OFF"}
                </span>
                <span className="glance-detail muted">{spaTranslate(tile.detail)}</span>
              </Link>
            </li>
          ))}
        </ul>
      </Panel>

      <Panel
        title={spaTranslate("MODULES")}
        aside={
          <PixelLink variant="ghost" className="button-small" href={`/g/${guild}/modules`}>
            {" "}
            {spaTranslate("Manage")}{" "}
          </PixelLink>
        }
      >
        <ul className="module-grid" aria-label={spaTranslate("Modules")}>
          {data.modules.map((m) => {
            const state = badge(m.enabled, m.channel_mode, m.channels.length);
            return (
              <li key={m.key}>
                <Link to={`/g/${guild}/modules`} className="module-tile" title={spaTranslate(m.blurb)}>
                  <span className="module-tile-name">{spaTranslate(m.label)}</span>
                  <span className={`badge badge-${state.tone}`}>{state.text}</span>
                </Link>
              </li>
            );
          })}
        </ul>
      </Panel>

      <Panel title={spaTranslate("RECENT CHANGES")}>
        {data.audit.length === 0 ? (
          <p className="muted">
            {" "}
            {spaTranslate(
              "No changes from the dashboard yet. What you save here shows up in this list.",
            )}{" "}
          </p>
        ) : (
          <ol className="audit-list">
            {data.audit.map((entry) => (
              <li key={entry.id} className="audit-row">
                <span className="audit-dot" aria-hidden="true" />
                <span className="audit-text">
                  <strong>{entry.actor_name}</strong> {describeChange(entry, names)}
                  {entry.source === "discord" && <span className="muted"> {spaTranslate("in Discord")}</span>}
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
