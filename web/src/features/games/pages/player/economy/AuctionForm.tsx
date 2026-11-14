import { useState } from "react";
import { PixelButton } from "../../../../../components/PixelButton";
import { t } from "../../../../../i18n";
import type { Economy } from "../../../../economy";
import type { Run } from "../../../../economy";

export function AuctionForm({ d, busy, run }: { d: Economy; busy: boolean; run: Run }) {
  const [item, setItem] = useState("");
  const [price, setPrice] = useState(100);
  const [hours, setHours] = useState(24);
  return (
    <form
      className="economy-form"
      onSubmit={(e) => {
        e.preventDefault();
        run({ action: "auction", item, price, hours });
      }}
    >
      <label>
        {t("Inventory item")}
        <select value={item} onChange={(e) => setItem(e.target.value)} required>
          <option value="">{t("Choose an item")}</option>
          {d.inventory.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name} ({i.quantity})
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Starting price")}
        <input
          type="number"
          min={1}
          max={10000000}
          value={price}
          onChange={(e) => setPrice(e.target.valueAsNumber)}
          required
        />
      </label>
      <label>
        {t("Hours")}
        <input
          type="number"
          min={1}
          max={72}
          value={hours}
          onChange={(e) => setHours(e.target.valueAsNumber)}
          required
        />
      </label>
      <PixelButton type="submit" disabled={busy || !item}>
        {t("List auction")}
      </PixelButton>
    </form>
  );
}
