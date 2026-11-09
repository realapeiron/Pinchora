import { Link, Outlet, useLocation } from "react-router";
import { Panel } from "../../../components/Panel";
import { translate as t } from "../../../i18n";
import { canOpenPage, type AccessOverview } from "../api/server-tools";
import type { NavGroup } from "../../../lib/nav";
import { SettingsPage } from "../../../components/PageHead";

/** The shell never renders a form for an area the extractor will refuse. */
export function ServerToolsOutlet({ data, nav }: { data: AccessOverview; nav: NavGroup[] }) {
  const { pathname } = useLocation();
  const path = pathname.slice(`/g/${data.guild.id}`.length).replace(/^\/+|\/+$/g, "");
  if (!canOpenPage(data, path))
    return (
      <SettingsPage
        title={t("Area not granted")}
        lead={t("Ask a server manager to grant your role access to this area.")}
      >
        <Link to={`/g/${data.guild.id}`}>{t("Overview")}</Link>
      </SettingsPage>
    );
  if (!path && data.access && !data.access.full) {
    return (
      <SettingsPage title={t("Overview")} lead={t("Your dashboard areas for this server.")}>
        <Panel title={t("Allowed areas")}>
          <ul className="list">
            {nav
              .flatMap((g) => g.pages)
              .filter((p) => p.to && canOpenPage(data, p.to))
              .map((p) => (
                <li className="list-row" key={p.to}>
                  <Link to={p.to}>{t(p.label)}</Link>
                </li>
              ))}
          </ul>
        </Panel>
      </SettingsPage>
    );
  }
  return <Outlet context={data} />;
}
