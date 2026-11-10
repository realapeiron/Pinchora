import { useState } from "react";
import { Link } from "react-router";
import { Icon } from "../../../components/icons/Icon";
import { badge } from "../api/modules";
import { PixelToggle } from "../../../components/PixelToggle";
import { Toast } from "../../../components/Toast";
import { t } from "../../../i18n/dashboard";
import type { ModuleEntry } from "../../../lib/api";
import { moduleIcons } from "./moduleIcons";
import { modulePage } from "./navModules";
import { catalogFilter } from "./overviewModel";
import { useModuleToggle } from "./useModuleToggle";

const groupWords: Record<string, string> = {
  server: "Server tools",
  players: "Players",
  home: "Home, pets and fishing",
  games: "Games",
  roles: "Roles",
  safety: "Safety",
  community: "Community",
  feeds: "Feeds",
};

export function OverviewCatalog({ guild, modules }: { guild: string; modules: ModuleEntry[] }) {
  const [group, setGroup] = useState("all");
  const [search, setSearch] = useState("");
  const [onlyOn, setOnlyOn] = useState(false);
  const [showAll, setShowAll] = useState(false);
  const switcher = useModuleToggle(guild);
  const groups = [...new Set(modules.map((module) => module.group))];
  const visible = catalogFilter(modules, group, search, onlyOn);
  const shown = showAll || group !== "all" || search.trim() || onlyOn ? visible : visible.slice(0, 12);
  const count = (key: string) => modules.filter((module) => key === "all" || module.group === key).length;
  return (
    <section className="overview-catalog" id="catalog" aria-label={t("Module catalog")}>
      <div className="overview-catalog-head">
        <h2>{t("MODULES")}</h2>
        <div className="overview-catalog-filters">
          <label className="overview-search">
            <Icon name="search" />
            <span className="visually-hidden">{t("Search modules")}</span>
            <input
              type="search"
              value={search}
              placeholder={t("Search modules")}
              onChange={(event) => setSearch(event.target.value)}
            />
          </label>
          <label className="overview-only-on">
            <input type="checkbox" checked={onlyOn} onChange={(event) => setOnlyOn(event.target.checked)} />
            {t("Only switched on")}
          </label>
        </div>
      </div>
      <div className="overview-tabs" role="group" aria-label={t("Category")}>
        {["all", ...groups].map((key) => (
          <button key={key} type="button" aria-pressed={group === key} onClick={() => setGroup(key)}>
            {key === "all" ? t("All") : t(groupWords[key] ?? key)} <span>{count(key)}</span>
          </button>
        ))}
      </div>
      <div className="overview-modules">
        {shown.map((module) => {
          const page = modulePage(module.key);
          const href = page === "" ? `/g/${guild}/#text-prefix` : `/g/${guild}/${page ?? "modules"}`;
          return (
            <article key={module.key} className={`overview-module ${module.enabled ? "" : "is-off"}`}>
              <span className="overview-module-icon">
                <Icon name={moduleIcons[module.key as keyof typeof moduleIcons]} />
              </span>
              <PixelToggle
                checked={module.enabled}
                onChange={() => switcher.toggle(module)}
                disabled={switcher.pending}
                label={`${t(module.label)} ${t("module")}`}
              />
              <h3>
                <Link to={href}>{t(module.label)}</Link>
              </h3>
              <p>{t(module.blurb)}</p>
              <Link className="overview-module-foot" to={href}>
                {module.enabled && module.channel_mode !== "all" && (
                  <span className="overview-module-scope">
                    {badge(module.enabled, module.channel_mode, module.channels.length).text}
                  </span>
                )}
                <span className="overview-module-go">
                  {page !== undefined ? t("Open settings") : t("Manage modules")}
                  <Icon name="chevron-right" />
                </span>
              </Link>
            </article>
          );
        })}
        {!visible.length && (
          <p className="overview-no-modules">{t("No modules match. Clear the filters and try again.")}</p>
        )}
      </div>
      {shown.length < visible.length && (
        <button className="overview-show-all" type="button" onClick={() => setShowAll(true)}>
          {t("Show all")} {visible.length}
        </button>
      )}
      <Toast
        message={switcher.notice}
        onDone={switcher.clear}
        action={switcher.undo ? { label: t("Undo"), onClick: switcher.undo } : undefined}
      />
    </section>
  );
}
