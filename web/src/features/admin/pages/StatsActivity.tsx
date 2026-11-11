import { translate as spaTranslate } from "../../../i18n";
import { Panel } from "../../../components/Panel";
import {
  DailyColumns,
  Heatmap,
  JoinsLeaves,
  NumbersTable,
  RankBars,
  StatTile,
} from "../../../components/StatsCharts";
import {
  type GuildStats,
  type PeriodStats,
  WEEKDAYS,
  change,
  dayLabel,
  minutes,
  thousands,
} from "../api/stats";

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
