import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { t, number, dateTime } from "../../../../i18n";
import { api } from "../../../../lib/api";
import { usePlayer, petRename } from "../../api/webPlayer";
import { PageHead } from "../../../shell/pages/PageHead";
import { Loading, Problem } from "../../../shell/pages/States";

type Item = {
  id: string;
  name: string;
  quantity: number;
  category: string;
  rarity_name: string;
  category_name: string;
  bound: boolean;
  placed: { room: number; count: number }[];
};
type Pet = {
  id: string;
  name: string;
  species_name: string;
  level: number;
  xp: number;
  active: boolean;
  status: string;
  needs: Record<string, number>;
  trip_dest: string | null;
};
type Listing = { id: string; item_id: string; name: string; price: number };
type Fish = { id: string; name: string; rarity_name: string; caught: number; best_cm: number | null };
type Quest = {
  kind: string;
  text: string;
  target: number;
  progress: number;
  reward: number;
  claimed: boolean;
};
type Note = { id: string; author_id: string; author_name: string; body: string; created_at: string };
type Friend = { id: string; name: string; since: string };

function useAction(path: string) {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: ({
      suffix = "",
      method = "POST",
      body,
    }: {
      suffix?: string;
      method?: "POST" | "PATCH" | "DELETE";
      body?: unknown;
    }) => api(`${path}${suffix}`, method, body),
    onSuccess: () => void cache.invalidateQueries({ queryKey: ["player"] }),
  });
}
function Feedback({ action }: { action: ReturnType<typeof useAction> }) {
  if (action.isError)
    return (
      <p role="alert" className="notice notice-heart">
        {action.error.message}
      </p>
    );
  if (action.isSuccess)
    return (
      <p role="status" className="notice">
        {t("Done")}
      </p>
    );
  return null;
}
function Empty() {
  return <p className="muted">{t("Nothing here yet.")}</p>;
}
function petStatus(value: string) {
  switch (value) {
    case "away":
      return t("Away on a trip");
    case "asleep":
      return t("Asleep");
    case "back":
      return t("Back with a souvenir");
    default:
      return t("Awake");
  }
}
function needName(value: string) {
  switch (value) {
    case "hunger":
      return t("Hunger");
    case "fun":
      return t("Fun");
    case "energy":
      return t("Energy");
    default:
      return t("Hygiene");
  }
}

export function Inventory() {
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");
  const [rarity, setRarity] = useState("");
  const [set, setSet] = useState("");
  const [page, setPage] = useState(1);
  const [recipient, setRecipient] = useState("");
  const [giftId, setGiftId] = useState("");
  const q = usePlayer<{
    items: Item[];
    total: number;
    categories: { id: string; name: string }[];
    rarities: { id: string; name: string }[];
    sets: { id: string; name: string }[];
  }>(
    `inventory?q=${encodeURIComponent(search)}&category=${encodeURIComponent(category)}&rarity=${encodeURIComponent(rarity)}&set=${encodeURIComponent(set)}&page=${page}`,
  );
  const action = useAction("/api/me/inventory/gift");
  const friends = usePlayer<{ items: Friend[] }>("friends");
  return (
    <>
      <PageHead
        title={t("Inventory")}
        lead={t("Search your furniture and give a spare piece to a friend.")}
      />
      <Panel title={t("Furniture collection")}>
        <div className="player-depth-filters">
          <label>
            {t("Search")}
            <input
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setPage(1);
              }}
            />
          </label>
          <label>
            {t("Category")}
            <select
              value={category}
              onChange={(e) => {
                setCategory(e.target.value);
                setPage(1);
              }}
            >
              <option value="">{t("All categories")}</option>
              {q.data?.categories.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Rarity")}
            <select
              value={rarity}
              onChange={(e) => {
                setRarity(e.target.value);
                setPage(1);
              }}
            >
              <option value="">{t("All rarities")}</option>
              {q.data?.rarities.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Furniture set")}
            <select
              value={set}
              onChange={(e) => {
                setSet(e.target.value);
                setPage(1);
              }}
            >
              <option value="">{t("All sets")}</option>
              {q.data?.sets.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        {q.isPending ? (
          <Loading />
        ) : q.isError ? (
          <Problem error={q.error} />
        ) : q.data.items.length ? (
          <ul className="player-depth-list">
            {q.data.items.map((i) => (
              <li key={i.id}>
                <strong>{i.name}</strong>
                <span>
                  {i.category_name} · {i.rarity_name} · ×{number(i.quantity)}
                </span>
                {i.placed.map((room) => (
                  <p key={room.room} className="muted">
                    {t("Placed in room {room}", { room: room.room })} · ×{number(room.count)}
                  </p>
                ))}
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
      <Panel title={t("Gift an item")}>
        <div className="player-depth-filters">
          <label>
            {t("Friend's Discord ID")}
            <input
              inputMode="numeric"
              list="gift-friends"
              value={recipient}
              onChange={(e) => setRecipient(e.target.value)}
            />
            <datalist id="gift-friends">
              {friends.data?.items.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </datalist>
          </label>
          <label>
            {t("Item")}
            <select value={giftId} onChange={(e) => setGiftId(e.target.value)}>
              <option value="">{t("Choose an item")}</option>
              {q.data?.items
                .filter((i) => !i.bound && i.quantity > 0)
                .map((i) => (
                  <option key={i.id} value={i.id}>
                    {i.name}
                  </option>
                ))}
            </select>
          </label>
          <PixelButton
            disabled={!recipient || !giftId || action.isPending}
            onClick={() => action.mutate({ body: { to_user_id: recipient, item_id: giftId } })}
          >
            {t("Send gift")}
          </PixelButton>
        </div>
        <Feedback action={action} />
      </Panel>
    </>
  );
}
export function Pets() {
  const q = usePlayer<{ items: Pet[]; trips: { id: string; name: string; hours: number; level: number }[] }>(
    "pets",
  );
  const action = useAction("/api/me/pets");
  const [names, setNames] = useState<Record<string, string>>({});
  const [destinations, setDestinations] = useState<Record<string, string>>({});
  return (
    <>
      <PageHead title={t("Pets")} lead={t("Look after your companions and watch their needs.")} />
      {q.isPending ? (
        <Loading />
      ) : q.isError ? (
        <Problem error={q.error} />
      ) : q.data.items.length ? (
        <div className="player-depth-grid">
          {q.data.items.map((p) => (
            <Panel key={p.id} title={p.name}>
              <img
                className="player-pet-card"
                src={`/api/me/pets/${p.id}/card.png?v=${p.xp}-${Object.values(p.needs).join("-")}`}
                alt={`${p.name} ${p.species_name}`}
              />
              <p>
                {p.species_name} · {t("Level")} {number(p.level)} {p.active ? `· ${t("Active")}` : ""}
              </p>
              <p className="muted">{petStatus(p.status)}</p>
              <div className="player-needs">
                {Object.entries(p.needs).map(([key, value]) => (
                  <label key={key}>
                    {needName(key)}
                    <progress value={value} max={100} />
                    {number(value)}
                  </label>
                ))}
              </div>
              <div className="player-depth-actions">
                {(["feed", "play", "bathe", "walk"] as const).map((v) => (
                  <PixelButton
                    key={v}
                    disabled={action.isPending || p.status === "away" || p.status === "asleep"}
                    onClick={() => action.mutate({ suffix: `/${p.id}/care`, body: { action: v } })}
                  >
                    {t(v)}
                  </PixelButton>
                ))}
                {p.status === "asleep" && (
                  <PixelButton onClick={() => action.mutate({ suffix: `/${p.id}/wake` })}>
                    {t("Wake")}
                  </PixelButton>
                )}
                {!p.active && (
                  <PixelButton onClick={() => action.mutate({ suffix: `/${p.id}/active` })}>
                    {t("Make active")}
                  </PixelButton>
                )}
              </div>
              <div className="player-depth-filters">
                <label>
                  {t("Pet name")}
                  <input
                    value={names[p.id] ?? p.name}
                    maxLength={20}
                    onChange={(e) => setNames({ ...names, [p.id]: e.target.value })}
                  />
                </label>
                <PixelButton
                  disabled={action.isPending || !petRename(p.name, names[p.id])}
                  onClick={() =>
                    action.mutate({
                      suffix: `/${p.id}`,
                      method: "PATCH",
                      body: petRename(p.name, names[p.id]),
                    })
                  }
                >
                  {t("Rename")}
                </PixelButton>
              </div>
              {p.status === "awake" && (
                <div className="player-depth-filters">
                  <label>
                    {t("Trip destination")}
                    <select
                      value={destinations[p.id] ?? ""}
                      onChange={(e) => setDestinations({ ...destinations, [p.id]: e.target.value })}
                    >
                      <option value="">{t("Trip destination")}</option>
                      {q.data.trips
                        .filter((v) => p.level >= v.level)
                        .map((v) => (
                          <option key={v.id} value={v.id}>
                            {v.name} · {v.hours} h
                          </option>
                        ))}
                    </select>
                  </label>
                  <PixelButton
                    disabled={!destinations[p.id] || action.isPending}
                    onClick={() => {
                      const trip = q.data.trips.find((v) => v.id === destinations[p.id]);
                      if (trip)
                        action.mutate({
                          suffix: `/${p.id}/trip`,
                          body: { dest: trip.id, hours: trip.hours },
                        });
                    }}
                  >
                    {t("Send on trip")}
                  </PixelButton>
                </div>
              )}
            </Panel>
          ))}
        </div>
      ) : (
        <Empty />
      )}
      {q.data?.items.some((p) => p.status === "back" || (p.status === "asleep" && p.trip_dest)) && (
        <PixelButton onClick={() => action.mutate({ suffix: "/claim" })}>{t("Claim souvenirs")}</PixelButton>
      )}
      <Feedback action={action} />
    </>
  );
}
export function Stall() {
  const q = usePlayer<{ items: Listing[] }>("stall");
  const [stockSearch, setStockSearch] = useState("");
  const [stockPage, setStockPage] = useState(1);
  const stock = usePlayer<{ items: Item[]; total: number }>(
    `inventory?q=${encodeURIComponent(stockSearch)}&page=${stockPage}`,
  );
  const action = useAction("/api/me/stall/listings");
  const [id, setId] = useState("");
  const [price, setPrice] = useState("");
  return (
    <>
      <PageHead title={t("Stall")} lead={t("List furniture for other players to buy in Discord.")} />
      <Panel title={t("Your listings")}>
        {q.isPending ? (
          <Loading />
        ) : q.isError ? (
          <Problem error={q.error} />
        ) : q.data.items.length ? (
          <ul className="player-depth-list">
            {q.data.items.map((i) => (
              <li key={i.id}>
                <strong>{i.name}</strong>
                <span>
                  {number(i.price)} {t("Shells")}
                </span>
                <PixelButton
                  disabled={action.isPending}
                  onClick={() => action.mutate({ suffix: `/${i.id}`, method: "DELETE" })}
                >
                  {t("Unlist")}
                </PixelButton>
              </li>
            ))}
          </ul>
        ) : (
          <Empty />
        )}
      </Panel>
      <Panel title={t("List an item")}>
        <div className="player-depth-filters">
          <label>
            {t("Search")}
            <input
              value={stockSearch}
              onChange={(e) => {
                setStockSearch(e.target.value);
                setStockPage(1);
              }}
            />
          </label>
          <label>
            {t("Item")}
            <select value={id} onChange={(e) => setId(e.target.value)}>
              <option value="">{t("Choose an item")}</option>
              {stock.data?.items
                .filter((i) => !i.bound && i.quantity > 0)
                .map((i) => (
                  <option key={i.id} value={i.id}>
                    {i.name}
                  </option>
                ))}
            </select>
          </label>
          <label>
            {t("Price in shells")}
            <input
              type="number"
              min={1}
              max={10000000}
              value={price}
              onChange={(e) => setPrice(e.target.value)}
            />
          </label>
          <PixelButton
            disabled={!id || !price || action.isPending}
            onClick={() => action.mutate({ body: { item_id: id, price: Number(price) } })}
          >
            {t("List for sale")}
          </PixelButton>
        </div>
        <div className="player-depth-filters">
          <PixelButton disabled={stockPage === 1} onClick={() => setStockPage(stockPage - 1)}>
            {t("Previous")}
          </PixelButton>
          <PixelButton
            disabled={!stock.data || stockPage * 24 >= stock.data.total}
            onClick={() => setStockPage(stockPage + 1)}
          >
            {t("Next")}
          </PixelButton>
        </div>
        <Feedback action={action} />
      </Panel>
    </>
  );
}
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
