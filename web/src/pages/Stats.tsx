import { translate as spaTranslate } from "../i18n";
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
    <div className="period-tabs" role="group" aria-label={spaTranslate("Period")}>
      {PERIODS.map((d) => (
        <button
          key={d}
          type="button"
          className="period-tab"
          aria-pressed={d === days}
          onClick={() => onChange(d)}
        >
          {d} {spaTranslate("days")}
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
        title={spaTranslate("STATS")}
        lead={spaTranslate(
          "What happens in your server, counted without reading anyone's messages. Days and hours are in UTC.",
        )}
        action={<PeriodTabs days={days} onChange={setDays} />}
      />
      {stats.isPending && <Loading label={spaTranslate("Counting")} />}
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
          <strong>{spaTranslate("Stats")}</strong> {spaTranslate("is switched off in")}{" "}
          <Link to={`/g/${guild}/modules`}>{spaTranslate("Modules")}</Link>
          {spaTranslate(": nothing new is counted and counter channels keep their last number.")}{" "}
        </p>
      )}
      {!stats.voice_counted && (
        <p className="notice" role="note">
          {" "}
          {spaTranslate("Voice time is counted where voice XP is on. Switch it on in")}{" "}
          <Link to={`/g/${guild}/leveling`}>{spaTranslate("Leveling")}</Link>{" "}
          {spaTranslate("to see it here.")}{" "}
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
          label={spaTranslate("MESSAGES")}
          value={thousands(t.messages)}
          delta={change(t.messages, p?.messages)}
          period={stats.days}
          note={
            t.active_members === null
              ? undefined
              : t.active_members === 0
                ? spaTranslate("Nobody took part yet")
                : `${thousands(t.active_members)} members took part`
          }
        />
        <StatTile
          label={spaTranslate("VOICE TIME")}
          value={minutes(t.voice_minutes)}
          delta={change(t.voice_minutes, p?.voice_minutes)}
          period={stats.days}
        />
        <StatTile
          label={spaTranslate("JOINS")}
          value={thousands(t.joins)}
          delta={change(t.joins, p?.joins)}
          period={stats.days}
        />
        <StatTile
          label={spaTranslate("LEAVES")}
          value={thousands(t.leaves)}
          delta={change(t.leaves, p?.leaves)}
          upIsGood={false}
          period={stats.days}
          note={spaTranslate("Net {arg1}{arg2} members", { arg1: net > 0 ? "+" : "", arg2: thousands(net) })}
        />
      </div>

      <Panel title={spaTranslate("MESSAGES PER DAY")}>
        <div className="chart-room">
          <DailyColumns days={stats.daily} value={(d) => d.messages} label={spaTranslate("messages")} />
        </div>
        <NumbersTable
          caption={spaTranslate("Each day's numbers")}
          columns={[
            spaTranslate("Day (UTC)"),
            spaTranslate("Messages"),
            spaTranslate("Voice minutes"),
            spaTranslate("Joins"),
            spaTranslate("Leaves"),
          ]}
          rows={rows}
        />
      </Panel>

      <div className="stats-grid-2">
        <Panel title={spaTranslate("JOINS AND LEAVES")}>
          <div className="chart-room">
            <JoinsLeaves days={stats.daily} />
          </div>
        </Panel>
        <Panel title={spaTranslate("VOICE TIME PER DAY")}>
          <div className="chart-room">
            <DailyColumns
              days={stats.daily}
              value={(d) => d.voice_minutes}
              label={spaTranslate("voice minutes")}
              format={minutes}
              height={200}
            />
          </div>
        </Panel>
      </div>

      <Panel title={spaTranslate("BUSIEST HOURS (UTC)")}>
        <Heatmap grid={stats.heatmap} />
        <NumbersTable
          caption={spaTranslate("Messages by weekday and hour, UTC")}
          columns={[
            spaTranslate("Day"),
            ...Array.from({ length: 24 }, (_, h) => `${String(h).padStart(2, "0")}h`),
          ]}
          rows={stats.heatmap.map((hours, d) => [spaTranslate(WEEKDAYS[d]!), ...hours])}
        />
      </Panel>

      <div className={showMembers && members ? "stats-grid-2" : undefined}>
        <Panel title={spaTranslate("TOP CHANNELS")}>
          <RankBars
            label={spaTranslate("Top channels")}
            empty={spaTranslate("Nobody has written anywhere yet.")}
            rows={stats.top_channels.map((c) => ({
              id: c.id ?? "other",
              name: c.id
                ? c.name
                  ? `#${c.name}`
                  : spaTranslate("A thread or a deleted channel")
                : spaTranslate("Other"),
              value: c.messages,
              shown: `${thousands(c.messages)} msgs`,
            }))}
          />
        </Panel>
        {showMembers && members && (
          <Panel title={spaTranslate("TOP MEMBERS")}>
            <RankBars
              label={spaTranslate("Top members")}
              empty={spaTranslate("Nobody has written yet. Members who hide from leaderboards are left out.")}
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
      ? spaTranslate("Put {count} where the number goes.")
      : template.length > (list?.template_max ?? 80)
        ? spaTranslate("Keep it to {arg1} characters.", { arg1: list?.template_max ?? 80 })
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
          problem:
            error instanceof Error ? error.message : spaTranslate("The preview could not be worked out."),
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
      title={spaTranslate("COUNTER CHANNELS")}
      aside={
        <span className="pixel-label muted">
          {spaTranslate("{count}/{max} USED", { count: counters.length, max })}
        </span>
      }
    >
      <p className="muted panel-lead">
        {" "}
        {spaTranslate("A channel whose name shows a live number, like")}{" "}
        <strong>{spaTranslate("Members: 1,234")}</strong>
        {spaTranslate(
          ". Pinchora renames it when the number changes, at most once every 10 minutes. A locked voice channel works best. Same as",
        )}{" "}
        <code>/integrations stats</code> {spaTranslate("in Discord.")}{" "}
      </p>
      {query.isPending && <Loading label={spaTranslate("Reading the counters")} />}
      {query.isError && <Problem error={query.error} />}
      {list &&
        (counters.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("No counter channels yet.")}</p>
        ) : (
          <ul className="counter-list" aria-label={spaTranslate("Counter channels")}>
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
                    <span>
                      {c.name
                        ? spaTranslate("Renamed {arg1}", { arg1: relative(c.renamed_at) })
                        : spaTranslate("Not renamed yet")}
                    </span>
                  </p>
                </div>
                <div className="add-actions">
                  <PixelButton variant="ghost" className="button-small" onClick={() => edit(c)}>
                    {spaTranslate("Edit")}
                  </PixelButton>
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={spaTranslate("Stop counting in {arg1}", {
                      arg1: channelName(discord.data, c),
                    })}
                    disabled={remove.isPending}
                    onClick={() =>
                      remove.mutate(c.channel_id, {
                        onSuccess: () => setToast(spaTranslate("Removed. The channel keeps its last name.")),
                      })
                    }
                  >
                    {spaTranslate("Remove")}
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
        <Field label={spaTranslate("Channel")} htmlFor="counter-channel" error={fieldError("channel_id")}>
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
        <Field label={spaTranslate("Counts")} htmlFor="counter-kind" error={fieldError("kind")}>
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
                {spaTranslate(k.label)}
              </option>
            ))}
          </select>
        </Field>
        {chosen?.needs_role && (
          <Field label={spaTranslate("Role")} htmlFor="counter-role" error={fieldError("role_id")}>
            <RolePicker id="counter-role" info={discord.data} value={role} onChange={setRole} />
          </Field>
        )}
        <div className="field-wide">
          <Field
            label={spaTranslate("Name")}
            htmlFor="counter-template"
            error={templateProblem ?? fieldError("template")}
            hint={
              <>
                {" "}
                {spaTranslate("Leave it empty for")} <strong>{chosen?.template ?? "Members: {count}"}</strong>
                {spaTranslate(". Write")} <code>{"{count}"}</code>{" "}
                {spaTranslate("where the number goes.")}{" "}
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
          {previewing ? spaTranslate("Asking Discord…") : spaTranslate("Preview name")}
        </PixelButton>
        <PixelButton
          disabled={!channel || full || !!templateProblem || add.isPending || (chosen?.needs_role && !role)}
          onClick={() =>
            add.mutate(
              { ...draft, channel_id: channel },
              {
                onSuccess: () => {
                  setToast(
                    existing
                      ? spaTranslate("Counter updated.")
                      : spaTranslate("Counter added. The channel is renamed in a moment."),
                  );
                  setChannel(null);
                  setTemplate("");
                  setPreview(null);
                },
              },
            )
          }
        >
          {existing ? spaTranslate("Update counter") : spaTranslate("Add counter")}
        </PixelButton>
        {preview && (
          <p role="status">
            {preview.name && (
              <>
                <span className="muted">{spaTranslate("It would read")} </span>
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
        {full && (
          <p className="muted">
            {spaTranslate("All")} {max} {spaTranslate("counters are in use: remove one to add another.")}
          </p>
        )}
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
  if (seconds < 60) return spaTranslate("just now");
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
    <Panel title={spaTranslate("SHARE")}>
      <SwitchRow
        title={spaTranslate("Public stats page")}
        control={
          <PixelToggle
            label={spaTranslate("Public stats page")}
            checked={isPublic}
            disabled={setPublic.isPending}
            onChange={(on) =>
              setPublic.mutate(on, {
                onSuccess: () =>
                  setToast(
                    on
                      ? spaTranslate("Anyone with the link can see it now.")
                      : spaTranslate("The page is private again."),
                  ),
              })
            }
          />
        }
      >
        {" "}
        {spaTranslate(
          "Anyone with the link sees the last 30 days: the charts, the busiest hours and channels. No member is named.",
        )}{" "}
      </SwitchRow>
      {setPublic.isError && (
        <p className="field-error" role="alert">
          {setPublic.error.message}
        </p>
      )}
      <div className="panel-foot">
        {isPublic && (
          <PixelLink href={link} variant="ghost" target="_blank" rel="noreferrer">
            {" "}
            {spaTranslate("Open the public page")}{" "}
          </PixelLink>
        )}
        <PixelButton variant="secondary" onClick={() => setCard(Date.now())}>
          {card ? spaTranslate("Draw it again") : spaTranslate("Preview the /stats card")}
        </PixelButton>
      </div>
      {card && (
        <figure className="card-preview">
          <img
            src={`/api/guilds/${guild}/stats/card?days=${days}&v=${card}`}
            alt={spaTranslate(
              "The /stats server card for the last {arg1} days, as Pinchora draws it in Discord",
              { arg1: days },
            )}
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
