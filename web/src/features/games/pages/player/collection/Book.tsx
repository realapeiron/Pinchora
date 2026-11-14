import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import { useCollectionAction } from "./useCollectionAction";

export type Entry = {
  id: string;
  name: string;
  kind: string;
  rarity: string;
  caught: number;
  kept: number;
  glimmer: number;
  habitat: string | null;
};
type SetProgress = { id: string; found: number; total: number; complete: boolean };
export type Book = {
  items: Entry[];
  found: number;
  total: number;
  shards: number;
  fragments: number;
  craft_cost: number;
  sets: SetProgress[];
  roll: { entry_id: string; name: string; glimmer: boolean; rolled_at: string; claimed: boolean } | null;
};

function habitatName(id: string) {
  switch (id) {
    case "shore":
      return t("Shore set");
    case "tidepool":
      return t("Tide pool set");
    case "kelp":
      return t("Kelp set");
    case "reef":
      return t("Reef set");
    default:
      return t("Deep water set");
  }
}
export function CollectionBook() {
  const q = usePlayer<Book>("collection/book");
  const action = useCollectionAction();
  const [recycle, setRecycle] = useState<{ entry: Entry; request: string } | null>(null);
  const [craftRequest, setCraftRequest] = useState<string | null>(null);
  const [kind, setKind] = useState("critter");
  return (
    <>
      <PageHead
        title={t("Tide Pool collection book")}
        lead={t("Discover critters, keep Glimmers, and recycle spare cards into shards.")}
      />
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : (
        <>
          <div className="player-depth-grid">
            <Panel title={t("Hourly critter roll")}>
              <p>
                {q.data.roll ? `${q.data.roll.glimmer ? "✦ " : ""}${q.data.roll.name}` : t("No roll yet.")}
              </p>
              <p>{t("Six rolls per day. Claim within ten minutes; one new roll each hour.")}</p>
              <div className="player-actions">
                <PixelButton
                  disabled={
                    action.isPending ||
                    (!!q.data.roll && Date.now() < Date.parse(q.data.roll.rolled_at) + 3_600_000)
                  }
                  onClick={() => action.mutate({ path: "roll", method: "POST" })}
                >
                  {t("Roll a card")}
                </PixelButton>
                <PixelButton
                  variant="secondary"
                  disabled={
                    action.isPending ||
                    !q.data.roll ||
                    q.data.roll.claimed ||
                    Date.now() > Date.parse(q.data.roll.rolled_at) + 600_000
                  }
                  onClick={() => action.mutate({ path: "roll/claim", method: "POST" })}
                >
                  {t("Claim roll")}
                </PixelButton>
              </div>
            </Panel>
            <Panel title={t("Book progress")}>
              <p>
                {number(q.data.found)} / {number(q.data.total)} {t("discovered")}
              </p>
              <progress aria-label={t("Book progress")} value={q.data.found} max={q.data.total} />
              <p>
                {number(q.data.shards)} {t("Shards")} · {number(q.data.fragments)} {t("Fragments")}
              </p>
              <PixelButton
                onClick={() => {
                  const request = craftRequest ?? crypto.randomUUID();
                  setCraftRequest(request);
                  action.mutate(
                    { path: "craft", method: "POST", body: { request_id: request } },
                    { onSuccess: () => setCraftRequest(null) },
                  );
                }}
                disabled={action.isPending || q.data.shards < q.data.craft_cost}
              >
                {t("Craft bound loot")} · {number(q.data.craft_cost)} {t("Shards")}
              </PixelButton>
            </Panel>
            <Panel title={t("Habitat sets")}>
              <ul className="player-depth-list">
                {q.data.sets.map((set) => (
                  <li key={set.id}>
                    <strong>{habitatName(set.id)}</strong>
                    <span>
                      {number(set.found)}/{number(set.total)}{" "}
                      {set.complete ? `✦ ${t("Complete: crafting discount")}` : ""}
                    </span>
                  </li>
                ))}
              </ul>
            </Panel>
          </div>
          <Panel title={t("Collected cards")}>
            <div className="player-actions">
              <PixelButton
                variant={kind === "critter" ? "primary" : "secondary"}
                onClick={() => setKind("critter")}
              >
                {t("Critters")}
              </PixelButton>
              <PixelButton
                variant={kind === "find" ? "primary" : "secondary"}
                onClick={() => setKind("find")}
              >
                {t("Finds")}
              </PixelButton>
            </div>
            <ul className="player-depth-list">
              {q.data.items
                .filter((e) => e.kind === kind)
                .map((e) => (
                  <li key={e.id} className={e.glimmer > 0 ? "collection-glimmer" : undefined}>
                    <strong>
                      {e.caught ? e.name : "???"}{" "}
                      {e.glimmer > 0 && <span aria-label={t("Glimmer variant")}>✦</span>}
                    </strong>
                    <span>
                      {e.rarity} · {number(e.caught)} {t("caught")} · {number(e.kept)} {t("held")}
                    </span>
                    {e.kept > e.glimmer + 1 && (
                      <PixelButton
                        variant="secondary"
                        onClick={() => setRecycle({ entry: e, request: crypto.randomUUID() })}
                        disabled={action.isPending}
                      >
                        {t("Recycle spare")}
                      </PixelButton>
                    )}
                  </li>
                ))}
            </ul>
          </Panel>
          {recycle && (
            <div role="dialog" aria-modal="false" aria-label={t("Confirm recycle")} className="notice">
              <p>
                {recycle.entry.name} — {t("Recycle one spare? This cannot be undone.")}
              </p>
              <PixelButton
                disabled={action.isPending}
                onClick={() =>
                  action.mutate(
                    {
                      path: "recycle",
                      method: "POST",
                      body: { entry_id: recycle.entry.id, confirmed: true, request_id: recycle.request },
                    },
                    { onSuccess: () => setRecycle(null) },
                  )
                }
              >
                {t("Confirm recycle")}
              </PixelButton>
              <PixelButton variant="secondary" disabled={action.isPending} onClick={() => setRecycle(null)}>
                {t("Cancel")}
              </PixelButton>
            </div>
          )}
          {action.isError && (
            <p role="alert" className="notice notice-heart">
              {action.error.message}
            </p>
          )}
          {action.isSuccess && (
            <p role="status" className="notice">
              {t("Collection updated.")}
            </p>
          )}
        </>
      )}
    </>
  );
}
