import "../../../styles/moderation-desk.css";
import { useOutletContext, useParams } from "react-router";
import { translate as t } from "../../../i18n";
import type { Overview } from "../../../lib/api";
import { useModerationDesk } from "../api/moderationDesk";
import { RequireLogin } from "../../../components/RequireLogin";
import { MemberAppeal } from "./appeal/MemberAppeal";
import { AppealCases } from "./appeal/AppealCases";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { Desk } from "./desk/Desk";

export function ModerationDesk() {
  const { guild } = useOutletContext<Overview>();
  const { query } = useModerationDesk(guild.id);
  return (
    <SettingsPage
      className="settings-page mod-parity"
      title={t("Moderation desk")}
      lead={t("Review member reports and appeals, and configure restrictions and role rules.")}
    >
      {query.isPending && <Loading />}
      {query.isError && <Problem error={query.error} />}
      {query.data && (
        <Desk
          guild={guild.id}
          saved={query.data.config}
          reports={query.data.queues.reports}
          appeals={query.data.queues.appeals}
        />
      )}
    </SettingsPage>
  );
}

export function AppealForm() {
  const { guild } = useParams();
  return <RequireLogin>{() => (guild ? <MemberAppeal /> : <AppealCases />)}</RequireLogin>;
}
