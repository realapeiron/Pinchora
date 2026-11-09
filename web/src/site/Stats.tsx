import { translate as spaTranslate } from "../i18n";
// The public stats pages: the whole bot's totals at /stats, and a server's last 30 days at
// /stats/:guildId when its managers made the page public (no member is named there).

import { useParams } from "react-router";

import { ServerIcon } from "../components/ServerIcon";
import { DailyColumns, NumbersTable, StatTile } from "../components/StatsCharts";
import { ApiError } from "../lib/api";
import {
  type DayRow,
  dayLabel,
  thousands,
  useGlobalStats,
  usePublicStats,
} from "../features/admin/api/stats";
import { Activity } from "../features/admin/pages/Stats";
import { Loading } from "../components/States";
import { InviteButton } from "./Buttons";
import "../styles/stats.css";

export function GlobalStatsPage() {
  const stats = useGlobalStats();
  const totals = stats.data?.totals;
  const days: DayRow[] = (stats.data?.history ?? []).map((d) => ({
    day: d.day,
    messages: d.messages,
    voice_minutes: 0,
    joins: 0,
    leaves: 0,
    commands: 0,
  }));
  return (
    <div className="wrap stats-site">
      <header className="page-intro">
        <p className="pixel-label eyebrow">{spaTranslate("STATS")}</p>
        <h1 className="section-title">{spaTranslate("Pinchora in numbers")}</h1>
        <p className="section-lead">
          {" "}
          {spaTranslate(
            "Every server, player, house and fish, counted straight from the bot's database and brought up to date every few minutes. Servers can open their own stats page from the dashboard.",
          )}{" "}
        </p>
      </header>
      {stats.isPending && <Loading label={spaTranslate("Counting")} />}
      {stats.isError && (
        <p className="notice" role="note">
          {" "}
          {spaTranslate("The numbers can't be read just now. Try again in a moment.")}{" "}
        </p>
      )}
      {totals && (
        <div className="settings-page">
          <div className="stat-tiles stat-tiles-6">
            <StatTile label={spaTranslate("SERVERS")} value={thousands(totals.servers)} />
            <StatTile label={spaTranslate("PLAYERS")} value={thousands(totals.players)} />
            <StatTile label={spaTranslate("HOUSES BUILT")} value={thousands(totals.houses)} />
            <StatTile label={spaTranslate("PETS ADOPTED")} value={thousands(totals.pets)} />
            <StatTile label={spaTranslate("FISH CAUGHT")} value={thousands(totals.fish)} />
            <StatTile
              label={spaTranslate("GAMES PLAYED")}
              value={thousands(totals.games)}
              note={spaTranslate("Counted once per player")}
            />
          </div>
          <section className="panel">
            <div className="panel-body">
              <header className="panel-head">
                <h2 className="pixel-title">{spaTranslate("MESSAGES COUNTED PER DAY")}</h2>
              </header>
              {days.length > 1 ? (
                <>
                  <div className="chart-room">
                    <DailyColumns days={days} value={(d) => d.messages} label={spaTranslate("messages")} />
                  </div>
                  <NumbersTable
                    caption={spaTranslate("Each day's totals")}
                    columns={[
                      spaTranslate("Day (UTC)"),
                      spaTranslate("Messages"),
                      spaTranslate("Servers"),
                      spaTranslate("Players"),
                    ]}
                    rows={(stats.data?.history ?? []).map((d) => [
                      dayLabel(d.day, true),
                      d.messages,
                      d.servers,
                      d.players,
                    ])}
                  />
                </>
              ) : (
                <p className="empty-row muted">
                  {spaTranslate("The day by day history starts today: come back tomorrow.")}
                </p>
              )}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

export function PublicServerStats() {
  const { guildId = "" } = useParams();
  const stats = usePublicStats(guildId);
  const hidden = stats.error instanceof ApiError && stats.error.status === 404;
  return (
    <div className="wrap stats-site">
      {stats.isPending && <Loading label={spaTranslate("Counting")} />}
      {(hidden || (stats.isError && !hidden)) && (
        <header className="page-intro">
          <p className="pixel-label eyebrow">{spaTranslate("SERVER STATS")}</p>
          <h1 className="section-title">
            {hidden ? spaTranslate("These stats are private") : spaTranslate("Something went wrong")}
          </h1>
          <p className="section-lead">
            {hidden
              ? spaTranslate(
                  "This server hasn't opened its stats page, or the link is wrong. Server managers can open it on the Stats page of the dashboard.",
                )
              : spaTranslate("The numbers can't be read just now. Try again in a moment.")}
          </p>
          <div className="stats-site-cta">
            <InviteButton />
          </div>
        </header>
      )}
      {stats.data && (
        <>
          <header className="page-intro stats-site-head">
            <ServerIcon
              id={stats.data.guild.id}
              name={stats.data.guild.name}
              url={stats.data.guild.icon_url}
              size={64}
            />
            <div>
              <p className="pixel-label eyebrow">{spaTranslate("SERVER STATS")}</p>
              <h1 className="section-title">{stats.data.guild.name}</h1>
              <p className="muted">
                {dayLabel(stats.data.from)} – {dayLabel(stats.data.to, true)}
                {spaTranslate(", UTC. Counted by Pinchora without reading anyone's messages.")}{" "}
              </p>
            </div>
          </header>
          <div className="settings-page">
            <Activity stats={stats.data} showMembers={false} />
          </div>
        </>
      )}
    </div>
  );
}
