import { useState } from "react";
import { PixelButton } from "../../../../../components/PixelButton";
import { t, number } from "../../../../../i18n";
import type { Economy, Offer } from "../../../../economy";

export function OfferEditor({
  label,
  value,
  onChange,
  inventory,
}: {
  label: string;
  value: Offer;
  onChange: (v: Offer) => void;
  inventory?: Economy["inventory"];
}) {
  const [item, setItem] = useState("");
  const [qty, setQty] = useState(1);
  return (
    <fieldset>
      <legend>{label}</legend>
      <label>
        {t("Shells")}
        <input
          type="number"
          min={0}
          max={10000000}
          value={value.shells}
          onChange={(e) => onChange({ ...value, shells: e.target.valueAsNumber })}
          required
        />
      </label>
      <ul>
        {value.items.map((i) => (
          <li key={i.item_id}>
            {number(i.quantity)} × {i.item_id}{" "}
            <PixelButton
              variant="ghost"
              onClick={() =>
                onChange({ ...value, items: value.items.filter((x) => x.item_id !== i.item_id) })
              }
            >
              {t("Remove")}
            </PixelButton>
          </li>
        ))}
      </ul>
      <label>
        {t("Item identifier")}
        {inventory ? (
          <select value={item} onChange={(e) => setItem(e.target.value)}>
            <option value="">{t("Choose an item")}</option>
            {inventory.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name}
              </option>
            ))}
          </select>
        ) : (
          <input value={item} maxLength={100} onChange={(e) => setItem(e.target.value)} />
        )}
      </label>
      <label>
        {t("Quantity")}
        <input type="number" min={1} max={10} value={qty} onChange={(e) => setQty(e.target.valueAsNumber)} />
      </label>
      <PixelButton
        variant="secondary"
        disabled={
          !item ||
          !Number.isInteger(qty) ||
          qty < 1 ||
          qty > 10 ||
          value.items.length >= 8 ||
          value.items.some((i) => i.item_id === item)
        }
        onClick={() => {
          onChange({ ...value, items: [...value.items, { item_id: item, quantity: qty }] });
          setItem("");
        }}
      >
        {t("Add item to offer")}
      </PixelButton>
    </fieldset>
  );
}
