import { useState } from "react";
import { PixelButton } from "../../../../../components/PixelButton";
import { t, number, dateTime } from "../../../../../i18n";
import type { Trade, Offer } from "../../../../economy";
import type { Run } from "../../../../economy";

export function TradeReview({ trade: tr, busy, run }: { trade: Trade; busy: boolean; run: Run }) {
  const [checked, setChecked] = useState(false);
  return (
    <section className="economy-review" aria-label={t("Exact trade review")}>
      <h3>{t("Check both sides before confirming")}</h3>
      <div className="economy-columns">
        {[
          [tr.user_a, tr.offer_a, tr.confirmed_a],
          [tr.user_b, tr.offer_b, tr.confirmed_b],
        ].map(([id, offer, confirmed]) => (
          <div key={String(id)}>
            <h4>{t("Player {id}", { id: String(id) })}</h4>
            <p>{t("{amount} shells", { amount: (offer as Offer).shells })}</p>
            <ul>
              {(offer as Offer).items.map((i) => (
                <li key={i.item_id}>
                  {number(i.quantity)} × {i.item_id}
                </li>
              ))}
            </ul>
            <p>{confirmed ? t("Confirmed") : t("Awaiting confirmation")}</p>
          </div>
        ))}
      </div>
      <p>{t("Expires: {time}", { time: dateTime(tr.expires_at) })}</p>
      {tr.status === "open" && (
        <>
          <label className="economy-check">
            <input type="checkbox" checked={checked} onChange={(e) => setChecked(e.target.checked)} />
            {t("I checked both player IDs, item quantities and shell amounts.")}
          </label>
          <div className="player-actions">
            <PixelButton
              disabled={busy || !checked}
              onClick={() => {
                run({ action: "confirm_trade", id: tr.id });
                setChecked(false);
              }}
            >
              {t("Confirm this exact trade")}
            </PixelButton>
            <PixelButton
              variant="secondary"
              disabled={busy}
              onClick={() => run({ action: "cancel_trade", id: tr.id })}
            >
              {t("Cancel and refund")}
            </PixelButton>
          </div>
        </>
      )}
    </section>
  );
}
