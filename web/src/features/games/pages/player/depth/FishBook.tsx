import { Panel } from "../../../../../components/Panel";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";

type Fish = { id: string; name: string; rarity_name: string; caught: number; best_cm: number | null };

export function FishBook() {
  const q = usePlayer<{ items: Fish[] }>("fish-book");
  return (
    <>
      <PageHead title={t("Fish book")} lead={t("Every fish you have discovered on the coast.")} />
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : (
        <Panel title={t("Collection")}>
          <ul className="player-depth-list">
            {q.data.items.map((f) => (
              <li key={f.id}>
                <strong>{f.caught ? f.name : "?"}</strong>
                <span>
                  {f.rarity_name} · {f.caught ? `${number(f.caught)} · ${f.best_cm} cm` : t("Undiscovered")}
                </span>
              </li>
            ))}
          </ul>
        </Panel>
      )}
    </>
  );
}
