import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { translate as t } from "../../../../i18n";
import { api } from "../../../../lib/api";
import { Panel } from "../../../../components/Panel";
import { PageHead } from "../../../../components/PageHead";
import { Loading, Problem } from "../../../../components/States";

export function AppealCases() {
  const cases = useQuery({
    queryKey: ["my-appeals"],
    queryFn: () =>
      api<{ cases: { guild: string; number: number; kind: string; reason: string | null }[] }>(
        "/api/appeals",
      ),
  });
  return (
    <main className="settings-page mod-parity">
      <PageHead
        title={t("Member appeals")}
        lead={t("Explain what happened and why staff should reconsider this case.")}
      />
      <a href="/api/appeals/export" download="moderation-data.json">
        {t("Download moderation data")}
      </a>
      {cases.isPending && <Loading />}
      {cases.isError && <Problem error={cases.error} />}
      {cases.data && (
        <Panel title={t("Your cases")}>
          <ul className="list">
            {cases.data.cases.map((c) => (
              <li key={`${c.guild}-${c.number}`} className="list-row">
                <Link to={`/appeals/${c.guild}/${c.number}`}>
                  #{c.number} · {t(c.kind)} · {c.guild}
                </Link>
                <p>{c.reason}</p>
              </li>
            ))}
          </ul>
          {!cases.data.cases.length && <p>{t("No cases are open for appeal.")}</p>}
        </Panel>
      )}
    </main>
  );
}
