import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, dateTime } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import { useAction } from "./useAction";
import { Feedback } from "./Feedback";
import { Empty } from "./Empty";

type Note = { id: string; author_id: string; author_name: string; body: string; created_at: string };

export function Guestbook() {
  const [page, setPage] = useState(1);
  const q = usePlayer<{ items: Note[]; total: number; blocks: { id: string; name: string }[] }>(
    `guestbook?page=${page}`,
  );
  const action = useAction("/api/me/guestbook");
  return (
    <>
      <PageHead title={t("Guestbook")} lead={t("Messages from visitors to your house.")} />
      <Panel title={t("Visitor notes")}>
        {q.isPending ? (
          <Loading />
        ) : q.isError ? (
          <Problem error={q.error} />
        ) : q.data.items.length ? (
          <ul className="player-depth-list">
            {q.data.items.map((n) => (
              <li key={n.id}>
                <strong>{n.author_name}</strong>
                <time dateTime={n.created_at}>{dateTime(n.created_at)}</time>
                <p>{n.body}</p>
                <div className="player-depth-actions">
                  <PixelButton onClick={() => action.mutate({ suffix: `/${n.id}`, method: "DELETE" })}>
                    {t("Delete note")}
                  </PixelButton>
                  <PixelButton onClick={() => action.mutate({ suffix: `/blocks/${n.author_id}` })}>
                    {t("Block signer")}
                  </PixelButton>
                </div>
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
      <Panel title={t("Blocked signers")}>
        {q.isPending ? (
          <Loading />
        ) : q.isError ? (
          <Problem error={q.error} />
        ) : q.data.blocks.length ? (
          <ul className="player-depth-list">
            {q.data.blocks.map((b) => (
              <li key={b.id}>
                {b.name}
                <PixelButton onClick={() => action.mutate({ suffix: `/blocks/${b.id}`, method: "DELETE" })}>
                  {t("Unblock")}
                </PixelButton>
              </li>
            ))}
          </ul>
        ) : (
          <Empty />
        )}
      </Panel>
      <Feedback action={action} />
    </>
  );
}
