import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { useOutletContext } from "react-router";
import { RichText } from "../../../components/RichText";
import type { Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import "../../../styles/roles.css";
import { AutoRoles } from "./AutoRoles";
import { ColorRoles } from "./ColorRoles";
import { SettingsPage } from "../../../components/PageHead";
import { RolePanels } from "./RolePanels";
import { Loading, Problem } from "../../../components/States";
import { useRoles } from "../api/roles";

export function Roles() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const discord = useDiscord(guild);
  const roles = useRoles(guild);
  if (roles.isPending) return <Loading label={deferredText("Loading roles")} />;
  if (roles.isError) return <Problem error={roles.error} />;
  const view = roles.data;
  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("ROLES")}
      lead={deferredText(
        "Roles members get on their own, panels where they pick their own, and colours for their names.",
      )}
    >
      {discord.data && !discord.data.bot.manage_roles && (
        <p className="notice notice-heart">
          <RichText text="The bot lacks **Manage Roles**, so it can't hand out any role yet." />
        </p>
      )}
      <AutoRoles guild={guild} info={discord.data} view={view} />
      <RolePanels guild={guild} info={discord.data} view={view} />
      <ColorRoles guild={guild} info={discord.data} view={view} />
    </SettingsPage>
  );
}
