import { useState } from "react";
import { PixelButton } from "../../../../../components/PixelButton";
import { t } from "../../../../../i18n";
import type { Economy, Offer } from "../../../../economy";
import { OfferEditor } from "./OfferEditor";
import type { Run } from "../../../../economy";

export function TradeForm({ d, busy, run }: { d: Economy; busy: boolean; run: Run }) {
  const [to, setTo] = useState("");
  const [give, setGive] = useState<Offer>({ shells: 0, items: [] });
  const [receive, setReceive] = useState<Offer>({ shells: 0, items: [] });
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        run({ action: "trade", to, give, receive });
      }}
    >
      <label>
        {t("Partner player ID")}
        <input
          value={to}
          inputMode="numeric"
          pattern="[1-9][0-9]{0,18}"
          required
          onChange={(e) => setTo(e.target.value)}
        />
      </label>
      <div className="economy-columns">
        <OfferEditor label={t("You give")} value={give} onChange={setGive} inventory={d.inventory} />
        <OfferEditor label={t("You request")} value={receive} onChange={setReceive} />
      </div>
      <PixelButton
        type="submit"
        disabled={
          busy ||
          !to ||
          to === d.user_id ||
          (give.shells === 0 && receive.shells === 0 && !give.items.length && !receive.items.length)
        }
      >
        {t("Reserve my offer and create trade")}
      </PixelButton>
    </form>
  );
}
