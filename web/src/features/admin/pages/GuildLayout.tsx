import { t as deferredText } from "../../../i18n/dashboard";
import { ServerToolsOutlet } from "../../moderation";
import { canOpenPage } from "../../moderation";
import { translate as spaTranslate } from "../../../i18n";
import { useEffect, useRef, useState } from "react";
import { NavLink, useLocation, useParams } from "react-router";

import { PixelLink } from "../../../components/PixelButton";
import { Panel } from "../../../components/Panel";
import { ToastProvider } from "../../../components/Toast";
import { GuildTopBar } from "./GuildTopBar";
import { ApiError, type Me } from "../../../lib/api";
import { configureGuild, translate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { isDiscordId } from "../../../lib/ids";
import { useGuilds, useOverview } from "../../../lib/queries";
import { Icon } from "../../../components/icons/Icon";
import { iconNames } from "../../../components/icons/map";
import { GuildPageContext } from "../../../components/GuildPageContext";
import { IconSprite } from "../../../components/icons/IconSprite";
import { ShellNav } from "./ShellNav";
import { NAV } from "./guildNav";
import { headerModule } from "./navModules";
import { ModuleMaster } from "./ModuleMaster";
import { navIcons } from "./navIcons";
import { CommandPalette } from "./CommandPalette";
import { OverviewActions } from "./OverviewActions";
import "../../../styles/dashboard-shell.css";
import "../../../styles/dashboard-components.css";
import "../../../styles/settings-pattern.css";
import { NotFound } from "../../../components/NotFound";
import { Loading, Problem } from "../../../components/States";

export { NAV } from "./guildNav";

/** The admin side of one server: sidebar with the server and its pages, the page on the right. */
export function GuildLayout({ me }: { me: Me }) {
  const { guildId = "" } = useParams();
  if (!isDiscordId(guildId)) return <NotFound />;
  return (
    <ToastProvider className="page shell-page">
      <ValidGuildLayout me={me} guildId={guildId} />
    </ToastProvider>
  );
}

function ValidGuildLayout({ me, guildId }: { me: Me; guildId: string }) {
  const overview = useOverview(guildId);
  const guilds = useGuilds();
  useEffect(() => configureGuild(overview.data?.language ?? null), [overview.data?.language]);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [serverOpenRequested, setServerOpenRequested] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const closeButton = useRef<HTMLButtonElement>(null);
  const drawerReturn = useRef<HTMLElement | null>(null);
  const paletteReturn = useRef<HTMLElement | null>(null);
  const tabSearch = useRef<HTMLButtonElement>(null);
  const { pathname } = useLocation();
  const openPage = pathname.split(`/g/${guildId}/`)[1]?.split("/")[0] ?? "";
  const groups = NAV.map((group) => ({
    ...group,
    pages: group.pages
      .filter((page) => canOpenPage(overview.data, page.to))
      .map((page) => ({
        ...page,
        label: translate(overview.data?.navigation?.[page.to] ?? page.label),
      })),
  })).filter((group) => group.pages.length);
  const currentGroup = groups.find((group) => group.pages.some((page) => page.to === openPage));
  const currentPage = currentGroup?.pages.find((page) => page.to === openPage);
  const currentModule = canOpenPage(overview.data, openPage)
    ? headerModule(openPage, overview.data?.modules ?? [])
    : undefined;
  const openDrawer = (servers = false) => {
    drawerReturn.current = document.activeElement as HTMLElement;
    setServerOpenRequested(servers);
    setDrawerOpen(true);
    window.setTimeout(() => closeButton.current?.focus(), 0);
  };
  const closeDrawer = () => {
    setDrawerOpen(false);
    setServerOpenRequested(false);
    drawerReturn.current?.focus();
  };
  const openPalette = (fromDrawer = false) => {
    paletteReturn.current = fromDrawer ? tabSearch.current : (document.activeElement as HTMLElement);
    if (fromDrawer) {
      setDrawerOpen(false);
      setServerOpenRequested(false);
    }
    setPaletteOpen(true);
  };
  useEffect(() => {
    setDrawerOpen(false);
  }, [pathname]);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      const typing = target.matches("input, textarea, select, [contenteditable='true']");
      if (
        ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") ||
        (event.key === "/" && !typing)
      ) {
        event.preventDefault();
        if (paletteOpen) setPaletteOpen(false);
        else openPalette(drawerOpen);
      }
      if (event.key === "Escape" && drawerOpen && !event.defaultPrevented) closeDrawer();
    };
    document.addEventListener("keydown", keydown);
    return () => document.removeEventListener("keydown", keydown);
  }, [drawerOpen, paletteOpen]);
  const navProps = {
    me,
    guildId,
    guildName: overview.data?.guild.name ?? "…",
    guildIcon: overview.data?.guild.icon_url ?? null,
    groups,
    modules: overview.data?.modules ?? [],
    guilds: guilds.data ?? [],
    openPage,
    closeDrawer,
    openSearch: () => openPalette(drawerOpen),
    drawerOpen,
    serverOpenRequested,
    closeButton,
  };
  return (
    <>
      <IconSprite />
      <div className="guild shell-guild">
        <ShellNav {...navProps} />
        {drawerOpen && (
          <button type="button" className="shell-scrim" aria-label={t("Close menu")} onClick={closeDrawer} />
        )}
        <div className="shell-main">
          <GuildTopBar
            me={me}
            guild={{
              id: guildId,
              name: overview.data?.guild.name ?? "…",
              group: currentGroup?.group ?? "SERVER",
              page: currentPage?.label ?? "",
              openDrawer: () => openDrawer(true),
            }}
          />
          <main className="content shell-content">
            <GuildPageContext
              value={{
                group: currentGroup?.group ?? "SERVER",
                icon: iconNames[(navIcons[openPage] ?? "blocks") as keyof typeof iconNames],
                master: currentModule && <ModuleMaster guild={guildId} module={currentModule} />,
                offNote: currentModule && !currentModule.enabled ? t("Module off note") : undefined,
              }}
            >
              {overview.isPending && <Loading label={deferredText("Opening the server")} />}
              {overview.isError && <GuildProblem error={overview.error} />}
              {overview.isSuccess && (
                <OverviewActions.Provider value={{ openPalette: () => openPalette() }}>
                  <ServerToolsOutlet data={overview.data} nav={NAV} />
                </OverviewActions.Provider>
              )}
            </GuildPageContext>
          </main>
        </div>
      </div>
      <nav className="shell-tabs" aria-label={t("Server settings")}>
        <NavLink end to={`/g/${guildId}`}>
          <Icon name="layout-dashboard" />
          {t("Overview")}
        </NavLink>
        {canOpenPage(overview.data, "modules") && (
          <NavLink to={`/g/${guildId}/modules`}>
            <Icon name="blocks" />
            {t("Modules")}
          </NavLink>
        )}
        {canOpenPage(overview.data, "cases") && (
          <NavLink to={`/g/${guildId}/cases`}>
            <Icon name="folder-open" />
            {t("Cases")}
          </NavLink>
        )}
        <button ref={tabSearch} type="button" onClick={() => openPalette()}>
          <Icon name="search" />
          {t("Search")}
        </button>
        <button type="button" onClick={() => openDrawer()}>
          <Icon name="menu" />
          {t("Menu")}
        </button>
      </nav>
      <CommandPalette
        guildId={guildId}
        groups={groups}
        modules={overview.data?.modules ?? []}
        overview={overview.data}
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        returnTo={paletteReturn.current}
      />
    </>
  );
}

function GuildProblem({ error }: { error: Error }) {
  if (error instanceof ApiError && error.code === "bot_missing") {
    return (
      <Panel title={deferredText("Pinchora is not here yet")} tone="gold">
        <p>{deferredText("Add the bot to this server first, then come back to set it up.")}</p>
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
      <Panel title={deferredText("Not your server")} tone="accent">
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
