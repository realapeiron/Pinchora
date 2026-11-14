import type { Dispatch, SetStateAction } from "react";
import type { Content } from "../../iso/content";
import type { RoomJson } from "../../iso/scene";
import { t } from "../i18n";
import type { Point } from "../manifest";
import type { InventoryRow } from "./useDecorateRoom";

export function InventorySelection({
  hand,
  setHand,
  selected,
  setSelected,
  setTile,
  setFlipped,
  pending,
  inventory,
  catalog,
  confirmed,
}: {
  hand: string;
  setHand: Dispatch<SetStateAction<string>>;
  selected: string;
  setSelected: Dispatch<SetStateAction<string>>;
  setTile: Dispatch<SetStateAction<Point>>;
  setFlipped: Dispatch<SetStateAction<boolean>>;
  pending: boolean;
  inventory: { items: InventoryRow[] } | undefined;
  catalog: Content | undefined;
  confirmed: Pick<RoomJson, "placements">;
}) {
  return (
    <>
      <label>
        {t("Inventory")}
        <select
          value={hand}
          disabled={pending}
          onChange={(e) => {
            setHand(e.target.value);
            setSelected("");
            setFlipped(false);
          }}
        >
          <option value="">{t("Choose an item")}</option>
          {inventory?.items.map((i) => (
            <option key={i.item_id} value={i.item_id}>
              {catalog?.item(i.item_id) ? catalog.itemName(catalog.item(i.item_id)!) : i.item_id} ×
              {i.quantity}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Furniture and actions")}
        <select
          value={selected}
          disabled={pending}
          onChange={(e) => {
            setSelected(e.target.value);
            setHand("");
            const p = confirmed.placements.find((p) => p.id === e.target.value);
            if (p) {
              setTile([p.x, p.y]);
              setFlipped(p.flipped);
            }
          }}
        >
          <option value="">{t("Choose an item")}</option>
          {confirmed.placements.map((p) => (
            <option key={p.id} value={p.id}>
              {catalog?.item(p.item_id) ? catalog.itemName(catalog.item(p.item_id)!) : p.item_id} ({p.x},{" "}
              {p.y})
            </option>
          ))}
        </select>
      </label>
    </>
  );
}
