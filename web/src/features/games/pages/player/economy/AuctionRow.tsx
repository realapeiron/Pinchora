import { useState } from "react";
import { PixelButton } from "../../../../../components/PixelButton";
import { t, translate, dateTime } from "../../../../../i18n";
import type { Auction } from "../../../../economy";
import type { Run } from "../../../../economy";

export function AuctionRow({ a, user, busy, run }: { a: Auction; user: string; busy: boolean; run: Run }) {
  const [amount, setAmount] = useState(a.minimum_bid);
  const ended = Date.now() >= Date.parse(a.ends_at);
  return (
    <article>
      <h3>
        {t("Auction #{id}", { id: a.id })} · {a.item_id}
      </h3>
      <p>
        {t("Seller: {id}", { id: a.seller_id })} · {translate(a.status)}
      </p>
      <p>
        {t("Current bid: {amount} shells", { amount: a.bid })} · {dateTime(a.ends_at)}
      </p>
      {a.status === "open" && (
        <div className="economy-form">
          {!ended && a.seller_id !== user && (
            <>
              <label>
                {t("Your bid")}
                <input
                  type="number"
                  min={a.minimum_bid}
                  max={10000000}
                  value={amount}
                  onChange={(e) => setAmount(e.target.valueAsNumber)}
                />
              </label>
              <PixelButton
                disabled={busy || !Number.isInteger(amount) || amount < a.minimum_bid || amount > 10000000}
                onClick={() => run({ action: "bid", id: a.id, amount })}
              >
                {t("Reserve bid")}
              </PixelButton>
            </>
          )}
          {ended ? (
            <PixelButton disabled={busy} onClick={() => run({ action: "settle_auction", id: a.id })}>
              {t("Settle auction")}
            </PixelButton>
          ) : (
            a.seller_id === user &&
            !a.bidder_id && (
              <PixelButton
                variant="secondary"
                disabled={busy}
                onClick={() => run({ action: "cancel_auction", id: a.id })}
              >
                {t("Cancel auction")}
              </PixelButton>
            )
          )}
        </div>
      )}
    </article>
  );
}
