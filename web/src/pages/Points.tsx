import { number as spaNumber, translate as spaTranslate } from "../i18n";
import { useState } from "react";
import { useOutletContext } from "react-router";

import { Field, SwitchRow } from "../components/Field";
import { Panel } from "../components/Panel";
import { RolePicker, RoleTag } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { RichText } from "../components/RichText";
import { SaveBar } from "../components/SaveBar";
import { Toast } from "../components/Toast";
import type { FieldError, Overview, Settings } from "../lib/api";
import { useDiscord, useStore } from "../lib/queries";
import {
  LIMITS,
  checkPoints,
  checkPointsEmoji,
  checkPointsName,
  checkPrice,
  checkStock,
  problem,
} from "../lib/rules";
import { useSettingsForm } from "../lib/useSettingsForm";
import { SettingsPage } from "./PageHead";

function check(value: Settings): (FieldError | null)[] {
  const out = [checkPointsName(value.points_name), checkPointsEmoji(value.points_emoji)];
  const range = checkPoints(value.points_min, value.points_max);
  return [...out, range].map((c) => (c.ok ? null : c));
}

const thousands = (n: number) => spaNumber(n);

export function Points() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const form = useSettingsForm(guild, data.settings, check);
  const { value, set, errors } = form;
  const number = (raw: string) => (raw === "" ? Number.NaN : Number(raw));

  return (
    <SettingsPage
      className="settings-page"
      title={spaTranslate("POINTS")}
      lead={spaTranslate(
        "A currency for this server only: members earn it by chatting and spend it on roles in /store.",
      )}
    >
      <Panel title={spaTranslate("SERVER POINTS")}>
        <SwitchRow
          title={spaTranslate("Earn points for chatting")}
          control={
            <PixelToggle
              label={spaTranslate("Earn points for chatting")}
              checked={value.points_enabled}
              onChange={(on) => set("points_enabled", on)}
            />
          }
        >
          {" "}
          {spaTranslate("Off keeps balances and the store; chatting just stops earning more.")}{" "}
        </SwitchRow>
        <div className="form-grid form-grid-2">
          <Field
            label={spaTranslate("Name")}
            htmlFor="points-name"
            error={errors.points_name}
            hint={spaTranslate("What members see, e.g. coins or shells.")}
          >
            <input
              id="points-name"
              className="pixel-input"
              value={value.points_name}
              maxLength={LIMITS.points_name_max + 8}
              onChange={(event) => set("points_name", event.target.value)}
              aria-invalid={errors.points_name ? true : undefined}
            />
          </Field>
          <Field
            label={spaTranslate("Emoji")}
            htmlFor="points-emoji"
            error={errors.points_emoji}
            hint={spaTranslate("One emoji, or a custom one as <:name:id>.")}
          >
            <input
              id="points-emoji"
              className="pixel-input input-short"
              value={value.points_emoji}
              onChange={(event) => set("points_emoji", event.target.value)}
              aria-invalid={errors.points_emoji ? true : undefined}
            />
          </Field>
          <Field label={spaTranslate("Least per message")} htmlFor="points-min" error={errors.points_min}>
            <input
              id="points-min"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.points[0]}
              max={LIMITS.points[1]}
              value={Number.isNaN(value.points_min) ? "" : value.points_min}
              onChange={(event) => set("points_min", number(event.target.value))}
              aria-invalid={errors.points_min ? true : undefined}
            />
          </Field>
          <Field label={spaTranslate("Most per message")} htmlFor="points-max" error={errors.points_max}>
            <input
              id="points-max"
              className="pixel-input input-short"
              type="number"
              min={LIMITS.points[0]}
              max={LIMITS.points[1]}
              value={Number.isNaN(value.points_max) ? "" : value.points_max}
              onChange={(event) => set("points_max", number(event.target.value))}
              aria-invalid={errors.points_max ? true : undefined}
            />
          </Field>
        </div>
        <p className="sample-line">
          <span className="pixel-label muted">{spaTranslate("LOOKS LIKE")}</span>
          <span>
            {" "}
            {spaTranslate("Each message earns")}{" "}
            <strong>
              {value.points_min}–{value.points_max}
            </strong>{" "}
            {value.points_emoji} {value.points_name}
            {spaTranslate(". XP multipliers scale it too.")}{" "}
          </span>
        </p>
      </Panel>

      <StoreItems guild={guild} emoji={data.settings.points_emoji} name={data.settings.points_name} />

      <SaveBar
        count={form.changes}
        saving={form.saving}
        blocked={form.blocked}
        error={form.barError}
        onSave={form.submit}
        onReset={form.reset}
      />
      <Toast message={form.toast} onDone={form.clearToast} />
    </SettingsPage>
  );
}

function StoreItems({ guild, emoji, name }: { guild: string; emoji: string; name: string }) {
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
    <Panel
      title={spaTranslate("STORE")}
      aside={
        <span className="pixel-label muted">{`${items.length}/${query.data?.max ?? LIMITS.max_store_items}`}</span>
      }
    >
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
    </Panel>
  );
}
