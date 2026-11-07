import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { ChannelPicker, RolePicker, channelLabel } from "../components/Pickers";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import {
  DailyColumns,
  Heatmap,
  JoinsLeaves,
  NumbersTable,
  RankBars,
  StatTile,
} from "../components/StatsCharts";
import { Toast } from "../components/Toast";
import { ApiError, type GuildInfo, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import {
  type Counter,
  type CounterPreview,
  type Counters,
  type Days,
  type GuildStats,
  type PeriodStats,
  PERIODS,
  WEEKDAYS,
  change,
  counterName,
  dayLabel,
  minutes,
  previewCounter,
  thousands,
  useCounters,
  useGuildStats,
  useSetPublic,
} from "../lib/stats";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/stats.css";

/** 7 / 30 / 90 days: one row above everything it scopes. */
export function PeriodTabs({ days, onChange }: { days: Days; onChange: (days: Days) => void }) {
  return (
    <div className="period-tabs" role="group" aria-label="Period">
      {PERIODS.map((d) => (
        <button
          key={d}
          type="button"
          className="period-tab"
          aria-pressed={d === days}
          onClick={() => onChange(d)}
        >
          {d} days
        </button>
      ))}
    </div>
  );
}

export function Stats() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const [days, setDays] = useState<Days>(30);
  const stats = useGuildStats(guild, days);
  return (
    <div className="settings-page stats-page">
      <PageHead
        title="STATS"
        lead="What happens in your server, counted without reading anyone's messages. Days and hours are in UTC."
        action={<PeriodTabs days={days} onChange={setDays} />}
      />
      {stats.isPending && <Loading label="Counting" />}
      {stats.isError && <Problem error={stats.error} />}
      {stats.data && (
        <div
          className={`settings-page ${stats.isPlaceholderData ? "is-refreshing" : ""}`}
          aria-busy={stats.isFetching}
        >
          <Notes guild={guild} stats={stats.data} />
          <Activity stats={stats.data} />
        </div>
      )}
      <CounterChannels guild={guild} />
      <Sharing guild={guild} days={days} isPublic={stats.data?.public ?? false} />
    </div>
  );
}

function Notes({ guild, stats }: { guild: string; stats: GuildStats }) {
  return (
    <>
      {!stats.module_on && (
        <p className="notice" role="note">
          <strong>Stats</strong> is switched off in <Link to={`/g/${guild}/modules`}>Modules</Link>: nothing
          new is counted and counter channels keep their last number.
        </p>
      )}
      {!stats.voice_counted && (
        <p className="notice" role="note">
          Voice time is counted where voice XP is on. Switch it on in{" "}
          <Link to={`/g/${guild}/leveling`}>Leveling</Link> to see it here.
        </p>
      )}
    </>
  );
}

/** The numbers and the charts of one period. */
export function Activity({ stats, showMembers = true }: { stats: PeriodStats; showMembers?: boolean }) {
  const t = stats.totals;
  const p = stats.previous;
  const net = t.joins - t.leaves;
  const rows = stats.daily.map((d) => [
    dayLabel(d.day, true),
    d.messages,
    d.voice_minutes,
    d.joins,
    d.leaves,
  ]);
  const members = "top_members" in stats ? (stats as GuildStats).top_members : null;
  return (
    <>
      <div className="stat-tiles">
        <StatTile
          label="MESSAGES"
          value={thousands(t.messages)}
          delta={change(t.messages, p?.messages)}
          period={stats.days}
          note={
            t.active_members === null
              ? undefined
              : t.active_members === 0
                ? "Nobody took part yet"
                : `${thousands(t.active_members)} members took part`
          }
        />
        <StatTile
          label="VOICE TIME"
          value={minutes(t.voice_minutes)}
          delta={change(t.voice_minutes, p?.voice_minutes)}
          period={stats.days}
        />
        <StatTile
          label="JOINS"
          value={thousands(t.joins)}
          delta={change(t.joins, p?.joins)}
          period={stats.days}
        />
        <StatTile
          label="LEAVES"
          value={thousands(t.leaves)}
          delta={change(t.leaves, p?.leaves)}
          upIsGood={false}
          period={stats.days}
          note={`Net ${net > 0 ? "+" : ""}${thousands(net)} members`}
        />
      </div>

      <Panel title="MESSAGES PER DAY">
        <div className="chart-room">
          <DailyColumns days={stats.daily} value={(d) => d.messages} label="messages" />
        </div>
        <NumbersTable
          caption="Each day's numbers"
          columns={["Day (UTC)", "Messages", "Voice minutes", "Joins", "Leaves"]}
          rows={rows}
        />
      </Panel>

      <div className="stats-grid-2">
        <Panel title="JOINS AND LEAVES">
          <div className="chart-room">
            <JoinsLeaves days={stats.daily} />
          </div>
        </Panel>
        <Panel title="VOICE TIME PER DAY">
          <div className="chart-room">
            <DailyColumns
              days={stats.daily}
              value={(d) => d.voice_minutes}
              label="voice minutes"
              format={minutes}
              height={200}
            />
          </div>
        </Panel>
      </div>

      <Panel title="BUSIEST HOURS (UTC)">
        <Heatmap grid={stats.heatmap} />
        <NumbersTable
          caption="Messages by weekday and hour, UTC"
          columns={["Day", ...Array.from({ length: 24 }, (_, h) => `${String(h).padStart(2, "0")}h`)]}
          rows={stats.heatmap.map((hours, d) => [WEEKDAYS[d]!, ...hours])}
        />
      </Panel>

      <div className={showMembers && members ? "stats-grid-2" : undefined}>
        <Panel title="TOP CHANNELS">
          <RankBars
            label="Top channels"
            empty="Nobody has written anywhere yet."
            rows={stats.top_channels.map((c) => ({
              id: c.id ?? "other",
              name: c.id ? (c.name ? `#${c.name}` : "A thread or a deleted channel") : "Other",
              value: c.messages,
              shown: `${thousands(c.messages)} msgs`,
            }))}
          />
        </Panel>
        {showMembers && members && (
          <Panel title="TOP MEMBERS">
            <RankBars
              label="Top members"
              empty="Nobody has written yet. Members who hide from leaderboards are left out."
              rows={members.map((m) => ({
                id: m.id,
                name: m.name ?? "Unknown member",
                value: m.messages,
                shown: `${thousands(m.messages)} msgs`,
                avatar: m.avatar_url,
              }))}
            />
          </Panel>
        )}
      </div>
    </>
  );
}

const KIND_LABEL = (list: Counters | undefined, key: string) =>
  list?.kinds.find((k) => k.key === key)?.label ?? key;

function channelName(info: GuildInfo | undefined, counter: Counter) {
  const found = info?.channels.find((c) => c.id === counter.channel_id);
  if (found) return channelLabel(found);
  return counter.channel_name ? `#${counter.channel_name}` : "Deleted channel";
}

function CounterChannels({ guild }: { guild: string }) {
  const discord = useDiscord(guild);
  const { query, add, remove } = useCounters(guild);
  const [channel, setChannel] = useState<string | null>(null);
  const [kind, setKind] = useState("members");
  const [role, setRole] = useState<string | null>(null);
  const [template, setTemplate] = useState("");
  const [preview, setPreview] = useState<CounterPreview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const list = query.data;
  const counters = list?.counters ?? [];
  const max = list?.max ?? 10;
  const chosen = list?.kinds.find((k) => k.key === kind);
  const existing = counters.find((c) => c.channel_id === channel);
  const full = counters.length >= max && !existing;
  const templateProblem =
    template.trim() && !template.includes("{count}")
      ? "Put {count} where the number goes."
      : template.length > (list?.template_max ?? 80)
        ? `Keep it to ${list?.template_max ?? 80} characters.`
        : null;
  const refused = add.error instanceof ApiError ? add.error : null;
  const fieldError = (field: string) => refused?.errors.find((e) => e.field === field)?.message ?? null;
  const draft = { kind, role_id: chosen?.needs_role ? role : null, template: template.trim() || null };

  const runPreview = () => {
    setPreviewing(true);
    previewCounter(guild, draft)
      .then(setPreview)
      .catch((error: unknown) =>
        setPreview({
          name: null,
          value: null,
          problem: error instanceof Error ? error.message : "The preview could not be worked out.",
        }),
      )
      .finally(() => setPreviewing(false));
  };

  const edit = (counter: Counter) => {
    setChannel(counter.channel_id);
    setKind(counter.kind);
    setRole(counter.role_id);
    setTemplate(counter.template);
    setPreview(null);
  };

  return (
    <Panel
      title="COUNTER CHANNELS"
      aside={<span className="pixel-label muted">{`${counters.length}/${max} USED`}</span>}
    >
      <p className="muted panel-lead">
        A channel whose name shows a live number, like <strong>Members: 1,234</strong>. Pinchora renames it
        when the number changes, at most once every 10 minutes. A locked voice channel works best. Same as{" "}
        <code>/integrations stats</code> in Discord.
      </p>
      {query.isPending && <Loading label="Reading the counters" />}
      {query.isError && <Problem error={query.error} />}
      {list &&
        (counters.length === 0 ? (
          <p className="empty-row muted">No counter channels yet.</p>
        ) : (
          <ul className="counter-list" aria-label="Counter channels">
            {counters.map((c) => (
              <li key={c.channel_id} className="counter-row">
                <div>
                  <p className="counter-name">
                    {c.name ??
                      (c.value !== null
                        ? counterName(c.template, c.value)
                        : c.template.replaceAll("{count}", "…"))}
                  </p>
                  <p className="counter-meta muted">
                    <span>{channelName(discord.data, c)}</span>
                    <span>{KIND_LABEL(list, c.kind)}</span>
                    <span>{c.name ? `Renamed ${relative(c.renamed_at)}` : "Not renamed yet"}</span>
                  </p>
                </div>
                <div className="add-actions">
                  <PixelButton variant="ghost" className="button-small" onClick={() => edit(c)}>
                    Edit
                  </PixelButton>
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={`Stop counting in ${channelName(discord.data, c)}`}
                    disabled={remove.isPending}
                    onClick={() =>
                      remove.mutate(c.channel_id, {
                        onSuccess: () => setToast("Removed. The channel keeps its last name."),
                      })
                    }
                  >
                    Remove
                  </PixelButton>
                </div>
                {c.problem && (
                  <p className="counter-problem notice notice-heart" role="note">
                    <RichText text={c.problem} />
                  </p>
                )}
              </li>
            ))}
          </ul>
        ))}

      <div className={`add-row add-row-counter ${chosen?.needs_role ? "add-row-counter-3" : ""}`}>
        <Field label="Channel" htmlFor="counter-channel" error={fieldError("channel_id")}>
          <ChannelPicker
            id="counter-channel"
            info={discord.data}
            value={channel}
            onChange={(id) => {
              setChannel(id);
              const there = counters.find((c) => c.channel_id === id);
              if (there) edit(there);
            }}
            any
            invalid={!!fieldError("channel_id")}
          />
        </Field>
        <Field label="Counts" htmlFor="counter-kind" error={fieldError("kind")}>
          <select
            id="counter-kind"
            className="pixel-input pixel-select"
            value={kind}
            onChange={(event) => {
              setKind(event.target.value);
              setPreview(null);
            }}
          >
            {(list?.kinds ?? []).map((k) => (
              <option key={k.key} value={k.key}>
                {k.label}
              </option>
            ))}
          </select>
        </Field>
        {chosen?.needs_role && (
          <Field label="Role" htmlFor="counter-role" error={fieldError("role_id")}>
            <RolePicker id="counter-role" info={discord.data} value={role} onChange={setRole} />
          </Field>
        )}
        <div className="field-wide">
          <Field
            label="Name"
            htmlFor="counter-template"
            error={templateProblem ?? fieldError("template")}
            hint={
              <>
                Leave it empty for <strong>{chosen?.template ?? "Members: {count}"}</strong>. Write{" "}
                <code>{"{count}"}</code> where the number goes.
              </>
            }
          >
            <input
              id="counter-template"
              className="pixel-input"
              value={template}
              maxLength={(list?.template_max ?? 80) + 10}
              placeholder={chosen?.template}
              onChange={(event) => {
                setTemplate(event.target.value);
                setPreview(null);
              }}
              aria-invalid={templateProblem ? true : undefined}
              aria-describedby="counter-template-hint"
            />
          </Field>
        </div>
      </div>
      <div className="counter-preview">
        <PixelButton
          variant="secondary"
          disabled={!!templateProblem || previewing || (chosen?.needs_role && !role)}
          onClick={runPreview}
        >
          {previewing ? "Asking Discord…" : "Preview name"}
        </PixelButton>
        <PixelButton
          disabled={!channel || full || !!templateProblem || add.isPending || (chosen?.needs_role && !role)}
          onClick={() =>
            add.mutate(
              { ...draft, channel_id: channel },
              {
                onSuccess: () => {
                  setToast(
                    existing ? "Counter updated." : "Counter added. The channel is renamed in a moment.",
                  );
                  setChannel(null);
                  setTemplate("");
                  setPreview(null);
                },
              },
            )
          }
        >
          {existing ? "Update counter" : "Add counter"}
        </PixelButton>
        {preview && (
          <p role="status">
            {preview.name && (
              <>
                <span className="muted">It would read </span>
                <span className="counter-preview-name">{preview.name}</span>
              </>
            )}
            {preview.problem && (
              <span className="muted">
                {" "}
                <RichText text={preview.problem} />
              </span>
            )}
          </p>
        )}
        {full && <p className="muted">All {max} counters are in use: remove one to add another.</p>}
      </div>
      {refused && !refused.field && (
        <p className="field-error" role="alert">
          <RichText text={refused.message} />
        </p>
      )}
      <Toast message={toast} onDone={clearToast} />
    </Panel>
  );
}

/** "3 min ago", from an ISO time. */
function relative(at: string | null): string {
  if (!at) return "never";
  const seconds = Math.max(0, (Date.now() - Date.parse(at)) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} days ago`;
}

function Sharing({ guild, days, isPublic }: { guild: string; days: Days; isPublic: boolean }) {
  const setPublic = useSetPublic(guild);
  const [card, setCard] = useState<number | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const link = `/stats/${guild}`;
  return (
    <Panel title="SHARE">
      <SwitchRow
        title="Public stats page"
        control={
          <PixelToggle
            label="Public stats page"
            checked={isPublic}
            disabled={setPublic.isPending}
            onChange={(on) =>
              setPublic.mutate(on, {
                onSuccess: () =>
                  setToast(on ? "Anyone with the link can see it now." : "The page is private again."),
              })
            }
          />
        }
      >
        Anyone with the link sees the last 30 days: the charts, the busiest hours and channels. No member is
        named.
      </SwitchRow>
      {setPublic.isError && (
        <p className="field-error" role="alert">
          {setPublic.error.message}
        </p>
      )}
      <div className="panel-foot">
        {isPublic && (
          <PixelLink href={link} variant="ghost" target="_blank" rel="noreferrer">
            Open the public page
          </PixelLink>
        )}
        <PixelButton variant="secondary" onClick={() => setCard(Date.now())}>
          {card ? "Draw it again" : "Preview the /stats card"}
        </PixelButton>
      </div>
      {card && (
        <figure className="card-preview">
          <img
            src={`/api/guilds/${guild}/stats/card?days=${days}&v=${card}`}
            alt={`The /stats server card for the last ${days} days, as Pinchora draws it in Discord`}
            width={900}
            height={1258}
            loading="lazy"
          />
        </figure>
      )}
      <Toast message={toast} onDone={clearToast} />
    </Panel>
  );
}
