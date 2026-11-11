import { number as spaNumber, translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { SettingsRow } from "../../../components/SettingsRow";
import { RolePicker, RoleTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import type { Settings } from "../../../lib/api";
import { useDiscord, useStore } from "../../../lib/queries";
import { LIMITS, checkPrice, checkStock, problem } from "../../../lib/rules";
import { t } from "../../../i18n/dashboard";
import { pointsSummary } from "./pointsSummary";
const thousands = (n: number) => spaNumber(n);

export function StoreItems({
  guild,
  emoji,
  name,
  settings,
}: {
  guild: string;
  emoji: string;
  name: string;
  settings: Settings;
}) {
  const discord = useDiscord(guild);
  const { query, write } = useStore(guild);
  const [role, setRole] = useState<string | null>(null);
  const [price, setPrice] = useState("100");
  const [stock, setStock] = useState("");
  const items = query.data?.items ?? [];
  const picked = discord.data?.roles.find((r) => r.id === role);
  const existing = items.find((i) => i.role_id === role);
  const priceProblem = price === "" ? spaTranslate("Write a price.") : problem(checkPrice(Number(price)));
  const stockProblem = stock === "" ? null : problem(checkStock(Number(stock)));
  const roleProblem = picked && !picked.assignable ? picked.why_not : null;

  const edit = (id: string) => {
    const item = items.find((i) => i.role_id === id);
    if (!item) return;
    setRole(item.role_id);
    setPrice(String(item.price));
    setStock(item.stock === null ? "" : String(item.stock));
  };

  return (
    <SettingsRow
      title={spaTranslate("STORE")}
      description={t("Manage roles members can buy.")}
      summary={pointsSummary(settings, items.length).store}
    >
      <div className="settings-detail-fields">
        <span className="pixel-label muted">{`${items.length}/${query.data?.max ?? LIMITS.max_store_items}`}</span>
        <p className="muted panel-lead">
          {" "}
          {spaTranslate("Roles members can buy with")} {name} {spaTranslate("in")} <code>/store</code>
          {spaTranslate(". The bot gives the role right away; members keep it when it goes off sale.")}{" "}
        </p>
        {items.length === 0 ? (
          <p className="empty-row muted">{spaTranslate("Nothing on sale yet.")}</p>
        ) : (
          <ul className="list" aria-label={spaTranslate("Store")}>
            {items.map((item) => (
              <li key={item.id} className="list-row">
                <span className="list-main">
                  <RoleTag info={discord.data} id={item.role_id} />
                </span>
                <span className="badge badge-gold">
                  {emoji} {thousands(item.price)}
                </span>
                <span className="list-note muted">
                  {item.stock === null ? "unlimited" : `${item.stock} left`}
                </span>
                <PixelButton variant="ghost" className="button-small" onClick={() => edit(item.role_id)}>
                  {spaTranslate("Edit")}
                </PixelButton>
                <PixelButton
                  variant="ghost"
                  className="button-small"
                  aria-label={spaTranslate("Take off sale")}
                  disabled={write.isPending}
                  onClick={() => write.mutate({ method: "DELETE", sub: `/${item.role_id}` })}
                >
                  {spaTranslate("Remove")}
                </PixelButton>
              </li>
            ))}
          </ul>
        )}
        <div className="add-row add-row-store">
          <Field label={spaTranslate("Role")} htmlFor="store-role" error={roleProblem}>
            <RolePicker id="store-role" info={discord.data} value={role} onChange={setRole} assignable />
          </Field>
          <Field
            label={spaTranslate("Price ({arg1})", { arg1: name })}
            htmlFor="store-price"
            error={priceProblem}
          >
            <input
              id="store-price"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.price[0]}
              value={price}
              onChange={(event) => setPrice(event.target.value)}
            />
          </Field>
          <Field label={spaTranslate("Stock")} htmlFor="store-stock" error={stockProblem}>
            <input
              id="store-stock"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.stock[0]}
              value={stock}
              placeholder={spaTranslate("∞ unlimited")}
              onChange={(event) => setStock(event.target.value)}
            />
          </Field>
          <div className="add-actions">
            <PixelButton
              disabled={!role || !!priceProblem || !!stockProblem || !!roleProblem || write.isPending}
              onClick={() =>
                role &&
                write.mutate(
                  {
                    method: "PUT",
                    sub: `/${role}`,
                    body: { price: Number(price), stock: stock === "" ? null : Number(stock) },
                  },
                  { onSuccess: () => setRole(null) },
                )
              }
            >
              {existing ? spaTranslate("Update") : spaTranslate("Put on sale")}
            </PixelButton>
          </div>
        </div>
        {write.isError && (
          <p className="field-error" role="alert">
            <RichText text={write.error.message} />
          </p>
        )}
      </div>
    </SettingsRow>
  );
}
