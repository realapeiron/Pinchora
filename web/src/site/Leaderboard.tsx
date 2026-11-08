import { useQuery } from "@tanstack/react-query";
import { useParams } from "react-router";
import { translate as tr, number as fmt } from "../i18n";
import { api } from "../lib/api";

type Row = { user_id: string; name: string | null; xp: number };
type Board = { guild_id: string; leaders: Row[]; weekly: Row[] };

export function PublicLeaderboard() {
  const { slug } = useParams();
  const board = useQuery({
    queryKey: ["public-leaderboard", slug],
    queryFn: () => api<Board>(`/api/public/leaderboard/${encodeURIComponent(slug ?? "")}`),
    enabled: !!slug,
  });
  return (
    <div className="wrap stats-site">
      <header className="page-intro">
        <p className="pixel-label eyebrow">{tr("LEVELS")}</p>
        <h1 className="section-title">{tr("Server leaderboard")}</h1>
        <p className="section-lead">
          {tr(
            "XP leaders and this week's top gainers. Members who hide their profile or leaderboard place are omitted.",
          )}
        </p>
      </header>
      {board.isPending && <p>{tr("Loading leaderboard…")}</p>}
      {board.isError && <p role="alert">{tr("This leaderboard is unavailable.")}</p>}
      {board.data && (
        <div className="settings-page">
          <section className="panel">
            <div className="panel-body">
              <h2 className="pixel-title">{tr("ALL TIME XP")}</h2>
              {board.data.leaders.length ? (
                <ol className="list">
                  {board.data.leaders.map((row) => (
                    <li className="list-row" key={row.user_id}>
                      <span className="list-main">{row.name ?? `${tr("Member")} ${row.user_id}`}</span>
                      <strong>{fmt(row.xp)} XP</strong>
                    </li>
                  ))}
                </ol>
              ) : (
                <p>{tr("No XP earned yet.")}</p>
              )}
            </div>
          </section>
          <section className="panel">
            <div className="panel-body">
              <h2 className="pixel-title">{tr("THIS WEEK")}</h2>
              {board.data.weekly.length ? (
                <ol className="list">
                  {board.data.weekly.map((row) => (
                    <li className="list-row" key={row.user_id}>
                      <span className="list-main">{row.name ?? `${tr("Member")} ${row.user_id}`}</span>
                      <strong>{fmt(row.xp)} XP</strong>
                    </li>
                  ))}
                </ol>
              ) : (
                <p>{tr("No XP earned this week yet.")}</p>
              )}
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
