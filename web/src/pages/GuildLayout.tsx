import { NavLink, Outlet, useParams } from "react-router";

import { PixelLink } from "../components/PixelButton";
import { Panel } from "../components/Panel";
import { ServerIcon } from "../components/ServerIcon";
import { TopBar } from "../components/TopBar";
import { ApiError, type Me } from "../lib/api";
import { useOverview } from "../lib/queries";
import { Loading, Problem } from "./States";

/** The admin side of one server: sidebar with the server and its pages, the page on the right. */
export function GuildLayout({ me }: { me: Me }) {
  const { guildId = "" } = useParams();
  const overview = useOverview(guildId);
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
                Switch server
              </NavLink>
            </div>
          </div>
          <nav aria-label="Server settings">
            <p className="sidebar-group pixel-label">SERVER</p>
            <NavLink end to={`/g/${guildId}`} className="sidebar-link">
              Overview
            </NavLink>
          </nav>
        </aside>
        <main className="content">
          {overview.isPending && <Loading label="Opening the server" />}
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
      <Panel title="Pinchora is not here yet" tone="gold">
        <p>Add the bot to this server first, then come back to set it up.</p>
        {error.inviteUrl && (
          <PixelLink href={error.inviteUrl} target="_blank" rel="noopener noreferrer">
            Add Pinchora
          </PixelLink>
        )}
      </Panel>
    );
  }
  if (error instanceof ApiError && error.code === "not_manager") {
    return (
      <Panel title="Not your server" tone="accent">
        <p>{error.message}</p>
        <PixelLink variant="secondary" href="/servers">
          Back to your servers
        </PixelLink>
      </Panel>
    );
  }
  return <Problem error={error} />;
}
