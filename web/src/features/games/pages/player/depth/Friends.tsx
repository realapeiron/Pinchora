import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, dateTime } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import type { Friend } from "./pageTypes";
import { Empty } from "./Empty";

export function Friends() {
  const [page, setPage] = useState(1);
  const q = usePlayer<{ items: Friend[]; total: number }>(`friends?page=${page}`);
  const partner = usePlayer<{ partner: { id: string; name: string; since: string } | null }>("partner");
  return (
    <>
      <PageHead title={t("Friends")} lead={t("Your friends and partner from Discord.")} />
      <Panel title={t("Partner")}>
        {partner.isPending ? (
          <Loading />
        ) : partner.isError ? (
          <Problem error={partner.error} />
        ) : partner.data.partner ? (
          <p>
            {partner.data.partner.name} · {dateTime(partner.data.partner.since)}
          </p>
        ) : (
          <Empty />
        )}
      </Panel>
      <Panel>
        {q.isPending ? (
          <Loading />
        ) : q.isError ? (
          <Problem error={q.error} />
        ) : q.data.items.length ? (
          <ul className="player-depth-list">
            {q.data.items.map((f) => (
              <li key={f.id}>
                <strong>{f.name || f.id}</strong>
                <span>{dateTime(f.since)}</span>
              </li>
            ))}
          </ul>
        ) : (
          <Empty />
        )}
        <div className="player-depth-filters">
          <PixelButton disabled={page === 1} onClick={() => setPage(page - 1)}>
            {t("Previous")}
          </PixelButton>
          <PixelButton disabled={!q.data || page * 24 >= q.data.total} onClick={() => setPage(page + 1)}>
            {t("Next")}
          </PixelButton>
        </div>
      </Panel>
    </>
  );
}
