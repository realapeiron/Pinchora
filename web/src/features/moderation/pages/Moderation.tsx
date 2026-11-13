import { t as deferredText } from "../../../i18n/dashboard";
import { useOutletContext } from "react-router";

import type { Overview } from "../../../lib/api";
import { useModeration } from "../../../lib/queries";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { ModerationForm } from "./ModerationForm";

export function Moderation() {
  const data = useOutletContext<Overview>();
  const { query } = useModeration(data.guild.id);
  return (
    <SettingsPage
      className="settings-page mod-page"
      title={deferredText("MODERATION")}
      lead={deferredText(
        "Where the cases of /ban, /kick, /timeout, /warn and the others are posted, whether members hear about them, and what enough warns bring.",
      )}
    >
      {query.isPending && <Loading label={deferredText("Reading the settings")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <ModerationForm guild={data.guild.id} saved={query.data} />}
    </SettingsPage>
  );
}
