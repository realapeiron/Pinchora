import { Panel } from "../../../../../components/Panel";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";

type Quest = {
  kind: string;
  text: string;
  target: number;
  progress: number;
  reward: number;
  claimed: boolean;
};

export function Quests() {
  const q = usePlayer<{ items: Quest[] }>("quests");
  return (
    <>
      <PageHead title={t("Quests")} lead={t("Today's progress. Claim rewards with /quests in Discord.")} />
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : (
        <div className="player-depth-grid">
          {q.data.items.map((v) => (
            <Panel key={v.kind} title={v.text}>
              <progress aria-label={v.text} value={v.progress} max={v.target} />
              <p>
                {number(v.progress)} / {number(v.target)} · {number(v.reward)} {t("Shells")}
              </p>
              {v.claimed && <p>{t("Claimed")}</p>}
            </Panel>
          ))}
        </div>
      )}
    </>
  );
}
