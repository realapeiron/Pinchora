import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";

import { useI18n } from "../i18n";
import { api } from "../lib/api";

export interface PublicStatus {
  servers: number;
  approximate_members: number;
  players: number;
  channels: number;
  shards: { id: number; total: number; latency_ms: number | null; guilds: number }[];
  uptime_seconds: number;
  version: string;
  rust_version: string;
  rss_mb: number;
  postgres_mb: number;
  redis_mb: number;
  commands_24h: number;
  commands_total: number;
  interactions_per_minute: number;
  active_games: number;
  open_tickets: number;
  invite_url: string;
  support_url: string;
  website_url: string;
  status_url: string;
}

export function useStatus() {
  return useQuery({
    queryKey: ["/api/public/status"],
    queryFn: () => api<PublicStatus>("/api/public/status"),
    staleTime: 30_000,
    retry: false,
  });
}

export function StatusTeaser() {
  const { data } = useStatus();
  const { t } = useI18n();
  return (
    <section className="band band-plain" aria-label={t("Bot status")}>
      <div className="wrap">
        <p className="pixel-label eyebrow">{t("BOT STATUS")}</p>
        <h2 className="section-title">{t("The reef is awake.")}</h2>
        <p className="section-lead">
          {data
            ? t("{servers} servers · ≈{members} members · {shards} shards", {
                servers: data.servers,
                members: data.approximate_members,
                shards: data.shards.length,
              })
            : t("Live bot health and reach.")}
        </p>
        <Link to="/status">{t("View live status →")}</Link>
      </div>
    </section>
  );
}

export function StatusPage() {
  const { t, number } = useI18n();
  const { data, isError } = useStatus();
  const values = data && [
    [t("Servers"), number(data.servers)],
    [t("Approx. members"), `≈${number(data.approximate_members)}`],
    [t("Players"), number(data.players)],
    [t("Channels"), number(data.channels)],
    [t("Shards"), number(data.shards.length)],
    [
      t("Uptime"),
      t("{hours}h {minutes}m", {
        hours: Math.floor(data.uptime_seconds / 3600),
        minutes: Math.floor(data.uptime_seconds / 60) % 60,
      }),
    ],
    [t("Process memory"), `${number(data.rss_mb)} MB`],
    [t("Postgres"), `${number(data.postgres_mb)} MB`],
    [t("Redis"), `${number(data.redis_mb)} MB`],
    [t("Commands / 24h"), number(data.commands_24h)],
    [t("Commands total"), number(data.commands_total)],
    [t("Interactions / min"), number(data.interactions_per_minute)],
    [t("Active games"), number(data.active_games)],
    [t("Open tickets"), number(data.open_tickets)],
    [t("Version"), data.version],
    [t("Rust"), data.rust_version],
  ];
  return (
    <div className="wrap status-page">
      <p className="pixel-label eyebrow">{t("PINCHORA / STATUS")}</p>
      <h1 className="section-title">{t("Bot status")}</h1>
      <p className="section-lead">{t("A live look at Pinchora across Discord.")}</p>
      {!data && (
        <p role="status">{isError ? t("Status is temporarily unavailable.") : t("Loading status…")}</p>
      )}
      {values && (
        <dl className="status-grid">
          {values.map(([label, value]) => (
            <div className="status-tile" key={label}>
              <dt className="pixel-label">{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      )}
      {data && (
        <>
          <h2>{t("Gateway latency")}</h2>
          <dl className="status-grid">
            {data.shards.map((shard) => (
              <div className="status-tile" key={shard.id}>
                <dt className="pixel-label">#{shard.id}</dt>
                <dd>{shard.latency_ms == null ? "—" : `${shard.latency_ms} ms`}</dd>
              </div>
            ))}
          </dl>
          <p className="status-links">
            <a href={data.invite_url}>{t("Invite")}</a> · <a href={data.website_url}>{t("Website")}</a>
            {data.support_url && (
              <>
                {" "}
                · <a href={data.support_url}>{t("Support server")}</a>
              </>
            )}
          </p>
        </>
      )}
    </div>
  );
}
