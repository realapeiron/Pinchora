import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { date } from "../../../i18n";
import { Panel } from "../../../components/Panel";
import { PixelButton } from "../../../components/PixelButton";
import { Loading, Problem } from "../../../components/States";
import { useInviteWords } from "../api/inviteWords";
import { type InvitePeriod, useInviteLeaders, useInviteMember } from "../api/invites";

export function Leaderboard({ guild }: { guild: string }) {
  const { t, n } = useInviteWords();
  const [period, setPeriod] = useState<InvitePeriod>("all");
  const [page, setPage] = useState(0);
  const [typed, setTyped] = useState("");
  const [member, setMember] = useState<string | null>(null);
  const leaders = useInviteLeaders(guild, period, page);
  const detail = useInviteMember(guild, member);
  const choose = (next: InvitePeriod) => {
    setPeriod(next);
    setPage(0);
  };
  return (
    <Panel
      title={deferredText("LEADERBOARD")}
      aside={
        <div className="inv-tabs" role="group" aria-label={deferredText("Leaderboard period")}>
          {(["all", "week", "month"] as const).map((key) => (
            <button type="button" key={key} aria-pressed={period === key} onClick={() => choose(key)}>
              {key === "all" ? deferredText("All time") : key === "week" ? t("7 days") : t("30 days")}
            </button>
          ))}
        </div>
      }
    >
      {leaders.isPending && <Loading label={deferredText("Reading leaderboard")} />}
      {leaders.isError && <Problem error={leaders.error} />}
      {leaders.data && (
        <>
          {leaders.data.rows.length === 0 ? (
            <p className="empty-row muted">{deferredText("No counted invitations in this period yet.")}</p>
          ) : (
            <div className="inv-table-wrap">
              <table className="inv-table">
                <thead>
                  <tr>
                    <th>{t("Rank")}</th>
                    <th>{t("Member")}</th>
                    <th>{deferredText("Regular")}</th>
                    <th>{t("Left")}</th>
                    <th>{deferredText("Too new")}</th>
                    <th>{t("Bonus")}</th>
                    <th>{t("Counted")}</th>
                  </tr>
                </thead>
                <tbody>
                  {leaders.data.rows.map((r, i) => (
                    <tr key={r.user_id}>
                      <td>{n(page * 25 + i + 1)}</td>
                      <td>
                        <button
                          className="inv-member-link"
                          onClick={() => {
                            setMember(r.user_id);
                            setTyped(r.user_id);
                          }}
                        >
                          {r.user_id}
                        </button>
                      </td>
                      <td>{n(r.regular)}</td>
                      <td>{n(r.left)}</td>
                      <td>{n(r.fake)}</td>
                      <td>{n(r.bonus)}</td>
                      <td className="inv-total">{n(r.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="inv-pagination">
            <PixelButton variant="secondary" disabled={page === 0} onClick={() => setPage(page - 1)}>
              {t("Previous")}
            </PixelButton>
            <span>{deferredText("Page {page}", { page: page + 1 })}</span>
            <PixelButton
              variant="secondary"
              disabled={leaders.data.rows.length < 25}
              onClick={() => setPage(page + 1)}
            >
              {t("Next")}
            </PixelButton>
          </div>
        </>
      )}
      <div className="inv-detail">
        <h3>{deferredText("Member detail")}</h3>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (/^\d{15,22}$/.test(typed)) setMember(typed);
          }}
        >
          <input
            className="pixel-input"
            aria-label={t("Discord user ID")}
            placeholder={t("Discord user ID")}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
          />
          <PixelButton type="submit">{deferredText("Look up")}</PixelButton>
        </form>
        {detail.isPending && member && <Loading label={deferredText("Reading member invites")} />}
        {detail.isError && <Problem error={detail.error} />}
        {detail.data && (
          <>
            <p className="inv-detail-total">
              {deferredText("{count} counted invites for {member}", {
                count: detail.data.score.total,
                member: member ?? "",
              })}
            </p>
            <ul className="inv-join-list">
              {detail.data.joins.map((j, i) => (
                <li key={`${j.user_id}-${i}`}>
                  <span>{j.user_id}</span>
                  <span>{date(j.joined_at)}</span>
                  <span className={`inv-source ${j.source}`}>
                    {j.source === "temporary" ? deferredText("possible used-up link") : t(j.source)}
                    {j.fake ? deferredText(" · new account") : ""}
                    {j.left_at ? deferredText(" · left") : ""}
                  </span>
                </li>
              ))}
            </ul>
            {detail.data.joins.length === 0 && (
              <p className="muted">{deferredText("No joins credited to this member.")}</p>
            )}
          </>
        )}
      </div>
    </Panel>
  );
}
