import { translate as spaTranslate } from "../i18n";
import { useEffect, useRef, useState } from "react";
import { NavLink, Outlet, useLocation, useParams } from "react-router";

import { PixelLink } from "../components/PixelButton";
import { Panel } from "../components/Panel";
import { ServerIcon } from "../components/ServerIcon";
import { TopBar } from "../components/TopBar";
import { ApiError, type Me } from "../lib/api";
import { configureGuild, t, translate } from "../i18n";
import { isDiscordId } from "../lib/ids";
import { type NavGroup, visibleNav } from "../lib/nav";
import { useOverview } from "../lib/queries";
import { NotFound } from "./NotFound";
import { Loading, Problem } from "./States";

/**
 * The admin pages of a server, by sidebar group. Paths are relative to `/g/:guildId`. A feature adds its
 * page as one line in its own group; a group with no page yet is not shown.
 */
export const NAV: NavGroup[] = [
  {
    group: "SERVER",
    pages: [
      { to: "", label: "Overview" },
      { to: "modules", label: "Modules" },
      { to: "command-control", label: "Commands" },
      { to: "backup", label: "Backup" },
      { to: "audit", label: "Audit log" },
      { to: "stats", label: "Stats" },
      { to: "invites", label: "Invites" },
      { to: "utility", label: "Messages" },
    ],
  },
  {
    group: "MEMBERS",
    pages: [
      { to: "welcome", label: "Welcome & goodbye" },
      { to: "welcome-image", label: "Welcome image" },
      { to: "leveling", label: "Leveling" },
      { to: "points", label: "Points & store" },
    ],
  },
  {
    group: "TOOLS",
    pages: [
      { to: "commands", label: "Custom commands" },
      { to: "responders", label: "Auto-responders" },
      { to: "community", label: "Community" },
    ],
  },
  { group: "ROLES", pages: [{ to: "roles", label: "Roles" }] },
  {
    group: "SAFETY",
    pages: [
      { to: "moderation", label: "Moderation" },
      { to: "safety", label: "Safety" },
      { to: "automod", label: "AutoMod" },
      { to: "anti-nuke", label: "Anti-nuke" },
      { to: "cases", label: "Cases" },
      { to: "log", label: "Server log" },
      { to: "staff-log", label: "Staff log" },
    ],
  },
  {
    group: "COMMUNITY",
    pages: [
      { to: "tickets", label: "Tickets" },
      { to: "events", label: "Events" },
      { to: "temp-voice", label: "Temp voice" },
      { to: "starboard", label: "Starboard" },
      { to: "birthdays", label: "Birthdays" },
    ],
  },
  {
    group: "FEEDS",
    pages: [
      { to: "feeds", label: "Social feeds" },
      { to: "trackers", label: "Game trackers" },
    ],
  },
  { group: "PLAYERS", pages: [{ to: "achievements", label: "Achievements" }] },
];

/** The admin side of one server: sidebar with the server and its pages, the page on the right. */
export function GuildLayout({ me }: { me: Me }) {
  const { guildId = "" } = useParams();
  if (!isDiscordId(guildId)) return <NotFound />;
  return <ValidGuildLayout me={me} guildId={guildId} />;
}

function ValidGuildLayout({ me, guildId }: { me: Me; guildId: string }) {
  const overview = useOverview(guildId);
  useEffect(() => configureGuild(overview.data?.language ?? null), [overview.data?.language]);
  const nav = useRef<HTMLElement>(null);
  const [filter, setFilter] = useState("");
  const shown = visibleNav(
    NAV.map((group) => ({
      ...group,
      pages: group.pages.map((page) => ({
        ...page,
        label: translate(overview.data?.navigation?.[page.to] ?? page.label),
      })),
    })),
    filter,
  );
  const { pathname } = useLocation();
  // On a phone the links are one sideways strip: keep the open page's tab in sight.
  useEffect(() => {
    const strip = nav.current;
    const active = strip?.querySelector(".sidebar-link.active");
    if (strip && active && strip.scrollWidth > strip.clientWidth) {
      strip.scrollLeft += active.getBoundingClientRect().left - strip.getBoundingClientRect().left - 16;
    }
  }, [pathname]);
  return (
    <div className="page">
      <TopBar me={me} />
      <div className="guild">
        <aside className="sidebar">
          <div className="sidebar-server">
            {overview.data ? (
              <ServerIcon
                id={guildId}
                name={overview.data.guild.name}
                url={overview.data.guild.icon_url}
                size={40}
              />
            ) : (
              <span className="server-icon server-icon-initials tile-edge" aria-hidden="true" />
            )}
            <div className="sidebar-server-text">
              <strong className="sidebar-server-name">{overview.data?.guild.name ?? "…"}</strong>
              <NavLink to="/servers" className="sidebar-switch">
                {t("Switch server")}
              </NavLink>
            </div>
          </div>
          <input
            type="search"
            className="pixel-input sidebar-filter"
            aria-label={t("Filter pages")}
            placeholder={t("Filter pages")}
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
          <nav ref={nav} aria-label={t("Server settings")} className="sidebar-nav">
            {shown.length === 0 && <p className="muted sidebar-empty">{t("No page matches.")}</p>}
            {shown.map(({ group, pages }) => (
              <div key={group} className="sidebar-section">
                <p className="sidebar-group pixel-label">{translate(group)}</p>
                {pages.map((page) => (
                  <NavLink
                    key={page.to}
                    end={page.to === ""}
                    to={page.to ? `/g/${guildId}/${page.to}` : `/g/${guildId}`}
                    className="sidebar-link"
                  >
                    {page.label}
                  </NavLink>
                ))}
              </div>
            ))}
          </nav>
        </aside>
        <main className="content">
          {overview.isPending && <Loading label={t("Opening the server")} />}
          {overview.isError && <GuildProblem error={overview.error} />}
          {overview.isSuccess && <Outlet context={overview.data} />}
        </main>
      </div>
    </div>
  );
}

function GuildProblem({ error }: { error: Error }) {
  if (error instanceof ApiError && error.code === "bot_missing") {
    return (
      <Panel title={t("Pinchora is not here yet")} tone="gold">
        <p>{t("Add the bot to this server first, then come back to set it up.")}</p>
        {error.inviteUrl && (
          <PixelLink href={error.inviteUrl} target="_blank" rel="noopener noreferrer">
            {t("Add Pinchora")}
          </PixelLink>
        )}
      </Panel>
    );
  }
  if (error instanceof ApiError && error.code === "not_manager") {
    return (
      <Panel title={spaTranslate("Not your server")} tone="accent">
        <p>{error.message}</p>
        <PixelLink variant="secondary" href="/servers">
          {" "}
          {spaTranslate("Back to your servers")}{" "}
        </PixelLink>
      </Panel>
    );
  }
  return <Problem error={error} />;
}
