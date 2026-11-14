import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import type { Item } from "./pageTypes";
import { useAction } from "./useAction";
import { Feedback } from "./Feedback";
import { Empty } from "./Empty";

type Listing = { id: string; item_id: string; name: string; price: number };

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
