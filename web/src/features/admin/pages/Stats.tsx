import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useState } from "react";
import { Link, useOutletContext } from "react-router";

import { SettingsRow } from "../../../components/SettingsRow";
import { t as dashboardT } from "../../../i18n/dashboard";
import { statsSummary } from "./statsSummary";

import { PixelButton, PixelLink } from "../../../components/PixelButton";

import { Toast } from "../../../components/Toast";
import { type Overview } from "../../../lib/api";

import { type Days, type GuildStats, PERIODS, useGuildStats, useSetPublic } from "../api/stats";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import "../../../styles/stats.css";
import { Activity } from "./StatsActivity";
import { CounterChannels } from "./StatsCounters";

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
    <SettingsPage
      className="settings-page stats-page"
      title={spaTranslate("STATS")}
      lead={spaTranslate(
        "What happens in your server, counted without reading anyone's messages. Days and hours are in UTC.",
      )}
      action={<PeriodTabs days={days} onChange={setDays} />}
    >
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
    </SettingsPage>
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
function Sharing({ guild, days, isPublic }: { guild: string; days: Days; isPublic: boolean }) {
  const setPublic = useSetPublic(guild);
  const [card, setCard] = useState<number | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const clearToast = useCallback(() => setToast(null), []);
  const link = `/stats/${guild}`;
  return (
    <div className="settings-detail-list">
      <SettingsRow
        title={spaTranslate("Public stats page")}
        description={dashboardT("Share a read-only stats page by link.")}
        summary={statsSummary(undefined, isPublic).publicPage}
        enabled={isPublic}
        onToggle={(on) =>
          setPublic.mutate(on, {
            onSuccess: () =>
              setToast(
                on
                  ? spaTranslate("Anyone with the link can see it now.")
                  : spaTranslate("The page is private again."),
              ),
          })
        }
        toggleDisabled={setPublic.isPending}
      >
        <div className="settings-detail-fields">
          <p>
            {spaTranslate(
              "Anyone with the link sees the last 30 days: the charts, the busiest hours and channels. No member is named.",
            )}
          </p>
          {setPublic.isError && (
            <p className="field-error" role="alert">
              {setPublic.error.message}
            </p>
          )}
          {isPublic && (
            <PixelLink href={link} variant="ghost" target="_blank" rel="noreferrer">
              {spaTranslate("Open the public page")}
            </PixelLink>
          )}
        </div>
      </SettingsRow>
      <SettingsRow
        title={spaTranslate("Preview the /stats card")}
        description={dashboardT("Preview the server stats card.")}
        summary={statsSummary(undefined, isPublic, card).card}
      >
        <div className="settings-detail-fields">
          <PixelButton variant="secondary" onClick={() => setCard(Date.now())}>
            {card ? spaTranslate("Draw it again") : spaTranslate("Preview the /stats card")}
          </PixelButton>
        </div>
        {card && (
          <figure className="card-preview settings-detail-preview">
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
      </SettingsRow>
      <Toast message={toast} onDone={clearToast} />
    </div>
  );
}

export { Activity } from "./StatsActivity";
