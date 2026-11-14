import { translate as t } from "../../../../i18n";
import { useEffect, useRef } from "react";
import type { Art } from "../../../../iso/art";
import type { HouseEditorModel } from "./useHouseEditor";

export function InventoryPanel({
  model,
}: {
  model: Pick<
    HouseEditorModel,
    | "search"
    | "setSearch"
    | "category"
    | "setCategory"
    | "rarity"
    | "setRarity"
    | "categories"
    | "rarities"
    | "categoryName"
    | "rarityName"
    | "inventory"
    | "stock"
    | "art"
    | "hand"
    | "choose"
  >;
}) {
  const {
    search,
    setSearch,
    category,
    setCategory,
    rarity,
    setRarity,
    categories,
    rarities,
    categoryName,
    rarityName,
    inventory,
    stock,
    art,
    hand,
    choose,
  } = model;
  return (
    <aside className="house-inventory">
      <h2>{t("Furniture")}</h2>
      <label>
        {t("Search inventory")}
        <input value={search} onChange={(e) => setSearch(e.target.value)} />
      </label>
      <label>
        {t("Category")}
        <select value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="">{t("All categories")}</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {categoryName(c)}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Rarity")}
        <select value={rarity} onChange={(e) => setRarity(e.target.value)}>
          <option value="">{t("All rarities")}</option>
          {rarities.map((r) => (
            <option key={r} value={r}>
              {rarityName(r)}
            </option>
          ))}
        </select>
      </label>
      {inventory.isError && <p role="alert">{t("Could not load inventory. Try again.")}</p>}
      <div className="house-stock">
        {stock.map((p) => {
          const item = art.content.item(p.item_id)!;
          return (
            <button
              type="button"
              key={p.item_id}
              disabled={p.quantity < 1}
              aria-pressed={hand === p.item_id}
              onClick={() => choose(p.item_id)}
              onPointerDown={() => choose(p.item_id)}
            >
              <ItemThumb art={art} id={p.item_id} />
              <span>
                {art.content.itemName(item)}
                <small>
                  ×{p.quantity}
                  {[
                    ...new Set(
                      inventory.data?.placed.filter((i) => i.item_id === p.item_id).map((i) => i.room),
                    ),
                  ]
                    .map((room) => ` · ${t("Room {n}", { n: room + 1 })}`)
                    .join("")}
                </small>
              </span>
            </button>
          );
        })}
        {!stock.length && <p>{t("No matching furniture.")}</p>}
      </div>
    </aside>
  );
}

function ItemThumb({ art, id }: { art: Art; id: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const sprite = art.item(id);
  useEffect(() => {
    if (sprite)
      ref.current
        ?.getContext("2d")
        ?.putImageData(new ImageData(sprite.data, sprite.width, sprite.height), 0, 0);
  }, [sprite]);
  return sprite ? <canvas ref={ref} width={sprite.width} height={sprite.height} aria-hidden="true" /> : null;
}
