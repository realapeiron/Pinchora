import { Panel } from "../components/Panel";
import { PixelButton, PixelLink } from "../components/PixelButton";
import { ServerIcon } from "../components/ServerIcon";
import { TopBar } from "../components/TopBar";
import { t } from "../i18n";
import type { GuildEntry, Me } from "../lib/api";
import { useMyChannels } from "../lib/feeds";
import { useGuilds, useRefreshGuilds } from "../lib/queries";
import { Loading, Problem } from "./States";

export function Servers({ me }: { me: Me }) {
  const guilds = useGuilds();
  const refresh = useRefreshGuilds();
  const channels = useMyChannels();
  return (
    <div className="page">
      <TopBar me={me} />
      <main className="content content-narrow">
        <div className="page-head">
          <div>
            <h1 className="pixel-heading">{t("YOUR SERVERS")}</h1>
            <p className="muted">{t("Servers where you have Manage Server. Pick one to set it up.")}</p>
          </div>
          <PixelButton variant="secondary" onClick={() => refresh.mutate()} disabled={refresh.isPending}>
            {refresh.isPending ? t("Refreshing…") : t("Refresh list")}
          </PixelButton>
        </div>
        {channels.data && (
          <Panel title={t("MY CHANNELS")}>
            <p>
              {t("Link your Twitch or Kick channel to receive a Live now role in servers that follow you.")}
            </p>
            <PixelLink href="/feeds/my-channels">{t("MY CHANNELS")}</PixelLink>
          </Panel>
        )}
        {refresh.isError && <p className="notice notice-heart">{refresh.error.message}</p>}
        {guilds.isPending && <Loading label={t("Finding your servers")} />}
        {guilds.isError && <Problem error={guilds.error} />}
        {guilds.isSuccess && guilds.data.length === 0 && (
          <Panel title={t("Nothing here yet")}>
            <p>
              {t(
                "You do not manage any servers. Ask a server owner for Manage Server, or create a server of your own on Discord, then refresh the list.",
              )}
            </p>
          </Panel>
        )}
        {guilds.isSuccess && guilds.data.length > 0 && (
          <ul className="server-grid">
            {guilds.data.map((guild) => (
              <ServerCard key={guild.id} guild={guild} />
            ))}
          </ul>
        )}
      </main>
    </div>
  );
}

function ServerCard({ guild }: { guild: GuildEntry }) {
  return (
    <li className={`panel ${guild.bot_present ? "panel-edge" : "panel-dim"} server-card`}>
      <div className="panel-body server-card-body">
        <ServerIcon id={guild.id} name={guild.name} url={guild.icon_url} />
        <div className="server-card-text">
          <h2 className="server-name" title={guild.name}>
            {guild.name}
          </h2>
          <span className="pixel-label muted">{guild.owner ? t("OWNER") : t("MANAGER")}</span>
        </div>
        {guild.bot_present ? (
          <PixelLink href={`/g/${guild.id}`} aria-label={t("Open {name}", { name: guild.name })}>
            {t("Open")}
          </PixelLink>
        ) : (
          <PixelLink
            variant="secondary"
            href={guild.invite_url ?? "#"}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={t("Add Pinchora to {name}", { name: guild.name })}
          >
            {t("Add bot")}
          </PixelLink>
        )}
      </div>
    </li>
  );
}
