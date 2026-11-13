import { t as deferredText } from "../../../i18n/dashboard";
import { useEffect, useRef, useState } from "react";
import { NavLink } from "react-router";
import { translate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import type { GuildEntry, Me, ModuleEntry } from "../../../lib/api";
import type { NavGroup } from "../../../lib/nav";
import { Icon } from "../../../components/icons/Icon";
import { navIcons } from "./navIcons";
import { GuildLanguageMenu } from "./GuildLanguageMenu";
import { navModule } from "./navModules";
import { trapTab } from "../../../components/overlayFocus";
import { ServerPicker } from "./ServerPicker";

function storedSet(key: string) {
  try {
    return new Set<string>(JSON.parse(localStorage.getItem(key) ?? "[]") as string[]);
  } catch {
    return new Set<string>();
  }
}
function persist(key: string, value: Set<string>) {
  try {
    localStorage.setItem(key, JSON.stringify([...value]));
  } catch {
    /* Private browsing. */
  }
}

export function ShellNav({
  me,
  guildId,
  guildName,
  guildIcon,
  groups,
  modules,
  guilds,
  openPage,
  closeDrawer,
  openSearch,
  drawerOpen,
  serverOpenRequested,
  closeButton,
}: {
  me: Me;
  guildId: string;
  guildName: string;
  guildIcon: string | null;
  groups: NavGroup[];
  modules: ModuleEntry[];
  guilds: GuildEntry[];
  openPage: string;
  closeDrawer: () => void;
  openSearch: () => void;
  drawerOpen: boolean;
  serverOpenRequested: boolean;
  closeButton: React.RefObject<HTMLButtonElement | null>;
}) {
  const sidebar = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!drawerOpen) return;
    const keydown = (event: KeyboardEvent) => {
      if (
        sidebar.current?.contains(document.activeElement) &&
        window.matchMedia("(max-width: 960px)").matches
      )
        trapTab(event, sidebar.current);
    };
    document.addEventListener("keydown", keydown);
    return () => document.removeEventListener("keydown", keydown);
  }, [drawerOpen]);
  const [collapsed, setCollapsed] = useState(() => storedSet("pinchora-nav-collapsed"));
  const [expanded, setExpanded] = useState(() => new Set<string>());
  const toggleGroup = (group: string) => {
    const next = new Set(collapsed);
    if (next.has(group)) next.delete(group);
    else next.add(group);
    setCollapsed(next);
    persist("pinchora-nav-collapsed", next);
  };
  return (
    <aside
      ref={sidebar}
      className={`sidebar shell-sidebar ${drawerOpen ? "is-open" : ""}`}
      aria-label={t("Server settings")}
    >
      <div className="shell-side-top">
        <div className="shell-brand">
          <img src="/mascot.png" alt="" />
          <span>PINCHORA</span>
          <button
            ref={closeButton}
            type="button"
            className="shell-close"
            aria-label={t("Close menu")}
            onClick={closeDrawer}
          >
            <Icon name="x" />
          </button>
        </div>
        <ServerPicker
          guildId={guildId}
          guildName={guildName}
          guildIcon={guildIcon}
          guilds={guilds}
          drawerOpen={drawerOpen}
          serverOpenRequested={serverOpenRequested}
          closeDrawer={closeDrawer}
        />
        <button
          type="button"
          className="shell-search"
          title={deferredText("Filter pages")}
          onClick={openSearch}
        >
          <Icon name="search" />
          <span>{t("Search pages")}</span>
          <kbd>{/Mac|iPhone|iPad|iPod/.test(navigator.platform) ? "⌘K" : "Ctrl K"}</kbd>
        </button>
      </div>
      <nav className="sidebar-nav shell-nav" aria-label={t("Server settings")}>
        {groups.map(({ group, pages }) => {
          const shut = collapsed.has(group) && !pages.some((page) => page.to === openPage);
          const limit =
            group === "COMMUNITY" && !expanded.has(group) && !pages.some((p) => p.to === openPage)
              ? 5
              : pages.length;
          return (
            <section className="shell-group" key={group}>
              <button
                type="button"
                className="shell-group-head"
                aria-expanded={!shut}
                onClick={() => toggleGroup(group)}
              >
                {translate(group)}
                <Icon name="chevron-down" />
              </button>
              {!shut && (
                <div className="shell-group-body">
                  {pages.slice(0, limit).map((page) => {
                    const mod = modules.find((m) => m.key === navModule(page.to));
                    return (
                      <NavLink
                        key={page.to}
                        end={page.to === ""}
                        to={page.to ? `/g/${guildId}/${page.to}` : `/g/${guildId}`}
                        title={mod?.blurb}
                        className="sidebar-link shell-nav-item"
                        onClick={closeDrawer}
                      >
                        <Icon name={navIcons[page.to] ?? "blocks"} />
                        <span className="shell-nav-label">{page.label}</span>
                        {mod && (
                          <span
                            className={`shell-dot ${mod.enabled ? "on" : "off"}`}
                            role="img"
                            aria-label={t(mod.enabled ? "On" : "Off")}
                          />
                        )}
                      </NavLink>
                    );
                  })}
                  {pages.length > limit && (
                    <button
                      type="button"
                      className="shell-more"
                      onClick={() => setExpanded(new Set([...expanded, group]))}
                    >
                      <Icon name="circle-plus" />
                      {t("{count} more pages", { count: pages.length - limit })}
                    </button>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </nav>
      <GuildLanguageMenu me={me} drawer />
      <div className="shell-side-foot">
        <NavLink to="/servers">{deferredText("Switch server")}</NavLink>
        <NavLink to="/me">{t("My world")}</NavLink>
      </div>
    </aside>
  );
}
