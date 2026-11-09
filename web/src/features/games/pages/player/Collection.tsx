import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { t, number } from "../../../../i18n";
import { api } from "../../../../lib/api";
import { usePlayer } from "../../api/webPlayer";
import { PageHead } from "../../../shell/pages/PageHead";
import { Loading, Problem } from "../../../shell/pages/States";

type Entry = {
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
type Book = {
  items: Entry[];
  found: number;
  total: number;
  shards: number;
  fragments: number;
  craft_cost: number;
  sets: SetProgress[];
  roll: { entry_id: string; name: string; glimmer: boolean; rolled_at: string; claimed: boolean } | null;
};
type Wish = { id: string; name: string; ping_on_spawn: boolean };

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

function useCollectionAction() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: ({
      path,
      method,
      body,
    }: {
      path: string;
      method: "POST" | "PUT" | "DELETE";
      body?: unknown;
    }) => api(`/api/me/collection/${path}`, method, body),
    onSuccess: () => void cache.invalidateQueries({ queryKey: ["player"] }),
  });
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

export function CollectionWishlist() {
  const book = usePlayer<Book>("collection/book");
  const wishes = usePlayer<{ items: Wish[]; max: number }>("collection/wishlist");
  const action = useCollectionAction();
  const [entry, setEntry] = useState("");
  const [ping, setPing] = useState(false);
  return (
    <>
      <PageHead
        title={t("Critter wishlist")}
        lead={t(
          "Choose up to ten discoveries. Spawn pings are optional; a matched card is reserved for 15 seconds.",
        )}
      />
      {book.isPending || wishes.isPending ? (
        <Loading />
      ) : book.isError ? (
        <Problem error={book.error} />
      ) : wishes.isError ? (
        <Problem error={wishes.error} />
      ) : (
        <>
          <Panel title={`${t("Wishlist")} · ${number(wishes.data.items.length)}/${number(wishes.data.max)}`}>
            <label className="collection-wishlist-select">
              {t("Choose a card")}
              <select
                value={entry}
                onChange={(e) => {
                  setEntry(e.target.value);
                  if (book.data.items.find((item) => item.id === e.target.value)?.kind === "find")
                    setPing(false);
                }}
              >
                <option value="">{t("Select a critter or find")}</option>
                {book.data.items
                  .filter((e) => !wishes.data.items.some((w) => w.id === e.id))
                  .map((e) => (
                    <option value={e.id} key={e.id}>
                      {e.name}
                    </option>
                  ))}
              </select>
            </label>
            <label>
              <input
                type="checkbox"
                checked={ping}
                disabled={book.data.items.find((item) => item.id === entry)?.kind === "find"}
                onChange={(e) => setPing(e.target.checked)}
              />{" "}
              {t("Ping me when this card spawns")}
            </label>
            <PixelButton
              disabled={!entry || action.isPending || wishes.data.items.length >= wishes.data.max}
              onClick={() => {
                action.mutate({
                  path: "wishlist",
                  method: "PUT",
                  body: { entry_id: entry, ping_on_spawn: ping },
                });
                setEntry("");
              }}
            >
              {t("Add wish")}
            </PixelButton>
            <ul className="player-depth-list">
              {wishes.data.items.map((w) => (
                <li key={w.id}>
                  <strong>{w.name}</strong>
                  <span>{w.ping_on_spawn ? t("Spawn ping on") : t("Spawn ping off")}</span>
                  <PixelButton
                    variant="secondary"
                    disabled={action.isPending}
                    onClick={() =>
                      action.mutate({ path: `wishlist/${encodeURIComponent(w.id)}`, method: "DELETE" })
                    }
                  >
                    {t("Remove")}
                  </PixelButton>
                  <PixelButton
                    variant="secondary"
                    disabled={
                      action.isPending || book.data.items.find((item) => item.id === w.id)?.kind === "find"
                    }
                    onClick={() =>
                      action.mutate({
                        path: "wishlist",
                        method: "PUT",
                        body: { entry_id: w.id, ping_on_spawn: !w.ping_on_spawn },
                      })
                    }
                  >
                    {t("Toggle ping")}
                  </PixelButton>
                </li>
              ))}
            </ul>
          </Panel>
          {action.isError && (
            <p role="alert" className="notice notice-heart">
              {action.error.message}
            </p>
          )}
          {action.isSuccess && (
            <p role="status" className="notice">
              {t("Wishlist updated.")}
            </p>
          )}
        </>
      )}
    </>
  );
}
