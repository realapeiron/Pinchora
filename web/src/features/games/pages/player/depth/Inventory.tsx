import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import type { Item, Friend } from "./pageTypes";
import { useAction } from "./useAction";
import { Feedback } from "./Feedback";
import { Empty } from "./Empty";

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
