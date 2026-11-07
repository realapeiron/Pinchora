import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";

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
  return (
    <section className="band band-plain" aria-label="Bot status">
      <div className="wrap">
        <p className="pixel-label eyebrow">BOT STATUS</p>
        <h2 className="section-title">The reef is awake.</h2>
        <p className="section-lead">
          {data
            ? `${data.servers.toLocaleString("en-US")} servers · ≈${data.approximate_members.toLocaleString("en-US")} members · ${data.shards.length} shards`
            : "Live bot health and reach."}
        </p>
        <Link to="/status">View live status →</Link>
      </div>
    </section>
  );
}

const labels = {
  en: {
    title: "Bot status",
    subtitle: "A live look at Pinchora across Discord.",
    servers: "Servers",
    members: "Approx. members",
    players: "Players",
    channels: "Channels",
    shards: "Shards",
    uptime: "Uptime",
    memory: "Process memory",
    postgres: "Postgres",
    redis: "Redis",
    commands24: "Commands / 24h",
    commandsTotal: "Commands total",
    interactions: "Interactions / min",
    games: "Active games",
    tickets: "Open tickets",
    version: "Version",
    rust: "Rust",
    latency: "Gateway latency",
    invite: "Invite",
    support: "Support server",
    website: "Website",
    loading: "Loading status…",
    unavailable: "Status is temporarily unavailable.",
  },
  tr: {
    title: "Bot durumu",
    subtitle: "Pinchora'nın Discord genelindeki canlı durumu.",
    servers: "Sunucular",
    members: "Yaklaşık üye",
    players: "Oyuncular",
    channels: "Kanallar",
    shards: "Parçalar",
    uptime: "Çalışma süresi",
    memory: "İşlem belleği",
    postgres: "Postgres",
    redis: "Redis",
    commands24: "Komutlar / 24 sa.",
    commandsTotal: "Toplam komut",
    interactions: "Etkileşim / dk.",
    games: "Aktif oyunlar",
    tickets: "Açık talepler",
    version: "Sürüm",
    rust: "Rust",
    latency: "Ağ gecikmesi",
    invite: "Davet et",
    support: "Destek sunucusu",
    website: "Web sitesi",
    loading: "Durum yükleniyor…",
    unavailable: "Durum şu anda alınamıyor.",
  },
};

export function StatusPage() {
  const [lang, setLang] = useState<"en" | "tr">(() => (document.documentElement.lang === "tr" ? "tr" : "en"));
  const l = labels[lang];
  const { data, isError } = useStatus();
  const number = (value: number) => value.toLocaleString(lang === "tr" ? "tr-TR" : "en-US");
  const values = data && [
    [l.servers, number(data.servers)],
    [l.members, `≈${number(data.approximate_members)}`],
    [l.players, number(data.players)],
    [l.channels, number(data.channels)],
    [l.shards, number(data.shards.length)],
    [l.uptime, `${Math.floor(data.uptime_seconds / 3600)}h ${Math.floor(data.uptime_seconds / 60) % 60}m`],
    [l.memory, `${number(data.rss_mb)} MB`],
    [l.postgres, `${number(data.postgres_mb)} MB`],
    [l.redis, `${number(data.redis_mb)} MB`],
    [l.commands24, number(data.commands_24h)],
    [l.commandsTotal, number(data.commands_total)],
    [l.interactions, number(data.interactions_per_minute)],
    [l.games, number(data.active_games)],
    [l.tickets, number(data.open_tickets)],
    [l.version, data.version],
    [l.rust, data.rust_version],
  ];
  return (
    <div className="wrap status-page">
      <p className="pixel-label eyebrow">PINCHORA / STATUS</p>
      <h1 className="section-title">{l.title}</h1>
      <p className="section-lead">{l.subtitle}</p>
      <div className="status-languages" aria-label="Language">
        <button type="button" aria-pressed={lang === "en"} onClick={() => setLang("en")}>
          EN
        </button>
        <button type="button" aria-pressed={lang === "tr"} onClick={() => setLang("tr")}>
          TR
        </button>
      </div>
      {!data && <p role="status">{isError ? l.unavailable : l.loading}</p>}
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
          <h2>{l.latency}</h2>
          <dl className="status-grid">
            {data.shards.map((shard) => (
              <div className="status-tile" key={shard.id}>
                <dt className="pixel-label">#{shard.id}</dt>
                <dd>{shard.latency_ms == null ? "—" : `${shard.latency_ms} ms`}</dd>
              </div>
            ))}
          </dl>
          <p className="status-links">
            <a href={data.invite_url}>{l.invite}</a> · <a href={data.website_url}>{l.website}</a>
            {data.support_url && (
              <>
                {" "}
                · <a href={data.support_url}>{l.support}</a>
              </>
            )}
          </p>
        </>
      )}
    </div>
  );
}
