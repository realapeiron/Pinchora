import { useState } from "react";
import { Link, useOutletContext } from "react-router";
import { Icon } from "../../../components/icons/Icon";
import { PixelButton, PixelLink } from "../../../components/PixelButton";
import { translate as t } from "../../../i18n";
import type { Overview as OverviewData } from "../../../lib/api";
import { useLevelRoles } from "../../../lib/queries";
import { compact, type Days, useGuildStats } from "../api/stats";
import { useOverviewActions } from "./OverviewActions";
import { OverviewCatalog } from "./OverviewCatalog";
import { OverviewChart } from "./OverviewChart";
import { OverviewExtras } from "./OverviewExtras";
import { attention, setupSteps, statCards, type CardKey } from "./overviewModel";
import "../../../styles/overview.css";

const cardWords: Record<CardKey, string> = {
  messages: "Messages",
  members: "Joined / left",
  commands: "Commands",
  active: "Active members",
};
const cardIcons: Record<CardKey, "message-square" | "users" | "square-terminal" | "trending-up"> = {
  messages: "message-square",
  members: "users",
  commands: "square-terminal",
  active: "trending-up",
};
const stepWords: Record<string, string> = {
  welcome: "Choose a welcome channel",
  goodbye: "Choose a goodbye channel",
  log: "Choose a server log channel",
  leveling: "Turn on leveling",
  stats: "Turn on stats",
};
const attentionWords: Record<string, string> = {
  welcome: "Welcome is on, but has no channel",
  goodbye: "Goodbye is on, but has no channel",
  log: "Server log is on, but has no channel",
  stats: "Stats is switched off",
  roles: "Level roles are empty",
};

export function Overview() {
  const data = useOutletContext<OverviewData>();
  const guild = data.guild.id;
  const [days, setDays] = useState<Days>(7);
  const stats = useGuildStats(guild, days);
  const roles = useLevelRoles(guild);
  const cards = statCards(stats.data);
  const signals = attention(
    data.settings,
    data.modules,
    roles.query.data?.items.length ?? null,
    stats.data?.module_on,
  );
  const steps = setupSteps(data.settings, data.modules);
  const done = steps.filter((step) => step.done);
  const left = steps.filter((step) => !step.done);
  const openPalette = useOverviewActions().openPalette;
  const onCount = data.modules.filter((module) => module.enabled).length;
  return (
    <div className="overview overview-redesign">
      <section className="overview-hero" aria-label={t("Server overview")}>
        <div className="overview-hero-art" role="img" aria-label={t("Pinchora Reef pixel town")} />
        <div className="overview-hero-copy">
          <h1>{data.guild.name}</h1>
          <p>
            <span className="overview-live-dot" aria-hidden="true" />
            {t("{count} modules switched on", { count: onCount })}
          </p>
          <div className="overview-hero-actions">
            <PixelLink href="/me">
              <Icon name="sailboat" />
              {t("Open my world")}
            </PixelLink>
            <PixelButton variant="secondary" onClick={openPalette}>
              <Icon name="search" />
              {t("Search settings")}
            </PixelButton>
          </div>
        </div>
      </section>

      {stats.isPending ? (
        <section className="overview-stats-empty" role="status">
          {t("Loading server activity…")}
        </section>
      ) : cards ? (
        <section
          className="overview-stats"
          aria-label={t("Activity for the last {count} days", { count: days })}
        >
          {cards.map((card) => (
            <article className="overview-stat" key={card.key}>
              <div className="overview-stat-label">
                <span className={`overview-stat-icon ${card.key}`}>
                  <Icon name={cardIcons[card.key]} />
                </span>
                {t(cardWords[card.key])}
              </div>
              <strong>
                {card.key === "members"
                  ? card.value
                  : card.value === "—"
                    ? card.value
                    : compact(Number(card.value))}
              </strong>
              <small>
                {card.change === null
                  ? t("Last {count} days", { count: days })
                  : t("{change}% vs previous period", {
                      change: `${card.change > 0 ? "+" : ""}${card.change}`,
                    })}
              </small>
            </article>
          ))}
        </section>
      ) : (
        <section className="overview-stats-empty" aria-label={t("Server activity")}>
          <p>
            {stats.isError
              ? t("Activity unavailable.")
              : t("No activity has been counted yet. Check Stats to start tracking.")}
          </p>
          <Link to={`/g/${guild}/stats`}>{t("Open Stats")}</Link>
        </section>
      )}

      <div className="overview-split">
        <section
          className={`overview-chart-panel ${cards ? "" : "is-empty"}`}
          aria-label={t("Messages chart")}
        >
          <div className="overview-section-head">
            <h2>{t("MESSAGES")}</h2>
            <div className="overview-range" role="group" aria-label={t("Period")}>
              {([7, 30] as const).map((period) => (
                <button
                  key={period}
                  type="button"
                  aria-pressed={days === period}
                  onClick={() => setDays(period)}
                >
                  {period} {t("days")}
                </button>
              ))}
            </div>
          </div>
          {cards ? (
            <OverviewChart rows={stats.data!.daily} />
          ) : (
            <div className="overview-chart-empty">
              <img src="/mascot.png" alt="" />
              <p>{t("No messages counted yet.")}</p>
              <Link to={`/g/${guild}/stats`}>{t("Stats")}</Link>
            </div>
          )}
        </section>
        <div className="overview-side">
          <section className="overview-panel overview-attention" aria-label={t("Attention")}>
            <div className="overview-section-head">
              <h2>{t("ATTENTION")}</h2>
              {signals.length > 0 && <span className="overview-count">{signals.length}</span>}
            </div>
            {signals.length ? (
              signals.map((signal) => (
                <div className="overview-attention-row" key={signal.key}>
                  <span className="overview-alert-icon">
                    <Icon
                      name={
                        signal.key === "roles"
                          ? "trending-up"
                          : signal.key === "stats"
                            ? "chart-line"
                            : "circle-alert"
                      }
                    />
                  </span>
                  <span>{t(attentionWords[signal.key]!)}</span>
                  <Link to={`/g/${guild}/${signal.page}`}>{t("Open settings")}</Link>
                </div>
              ))
            ) : (
              <p className="overview-all-clear">{t("All clear. Nothing needs attention.")}</p>
            )}
          </section>
          <section className="overview-panel overview-setup" aria-label={t("Setup")}>
            <div className="overview-section-head">
              <h2>{t("SETUP")}</h2>
              <span>
                {done.length} / {steps.length}
              </span>
            </div>
            <div
              className="overview-progress"
              role="img"
              aria-label={t("{done} of {total} setup steps complete", {
                done: done.length,
                total: steps.length,
              })}
            >
              {Array.from({ length: 10 }, (_, i) => (
                <span key={i} className={i < Math.round((done.length / steps.length) * 10) ? "on" : ""} />
              ))}
            </div>
            <p className="overview-done">
              <Icon name="check" />
              {done.length
                ? t("Done: {steps}", { steps: done.map((step) => t(stepWords[step.key]!)).join(", ") })
                : t("No setup steps complete yet.")}
            </p>
            {left.map((step, i) => (
              <Link
                className={`overview-task ${i === 0 ? "next" : ""}`}
                key={step.key}
                to={`/g/${guild}/${step.page}`}
              >
                <span className="overview-task-box" aria-hidden="true" />
                <span>{t(stepWords[step.key]!)}</span>
                {i === 0 ? <b>{t("Next")}</b> : <Icon name="chevron-right" />}
              </Link>
            ))}
            {!left.length && <p className="overview-all-clear">{t("Setup is complete.")}</p>}
          </section>
        </div>
      </div>
      <OverviewCatalog guild={guild} modules={data.modules} />
      <OverviewExtras data={data} />
    </div>
  );
}
