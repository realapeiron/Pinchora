import { useState } from "react";
import { Panel } from "../../../../../components/Panel";
import { PixelButton } from "../../../../../components/PixelButton";
import { PageHead } from "../../../../../components/PageHead";
import { Loading, Problem } from "../../../../../components/States";
import { t, number } from "../../../../../i18n";
import { usePlayer } from "../../../api/webPlayer";
import type { Book } from "./Book";
import { useCollectionAction } from "./useCollectionAction";

export type Wish = { id: string; name: string; ping_on_spawn: boolean };

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
