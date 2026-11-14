import { translate as t } from "../../../../i18n";
import type { HouseEditorModel } from "./useHouseEditor";
import { PixelButton } from "../../../../components/PixelButton";
import { tierLabel } from "../../api/house";
import type { EditOp } from "../../api/houseEdit";

export function HouseSurfaces({
  model,
}: {
  model: Pick<
    HouseEditorModel,
    | "target"
    | "room"
    | "pending"
    | "surfaces"
    | "send"
    | "setConfirm"
    | "surfaceName"
    | "house"
    | "notesOpen"
    | "setNotesOpen"
    | "notes"
  >;
}) {
  const {
    target,
    room,
    pending,
    surfaces,
    send,
    setConfirm,
    surfaceName,
    house,
    notesOpen,
    setNotesOpen,
    notes,
  } = model;
  return (
    <>
      {target.kind === "own" && (
        <details className="house-surfaces">
          <summary>{t("Walls, floors & house")}</summary>
          {(
            [
              ["wallpapers", "wallpaper", "Wallpaper"],
              ["floors", "floor", "Floor"],
            ] as const
          ).map(([key, field, label]) => (
            <label key={key}>
              {t(label)}
              <select
                value={room[field]}
                disabled={pending}
                onChange={(e) => {
                  const s = surfaces.data?.[key].find((s) => s.id === e.target.value);
                  if (!s) return;
                  const op: EditOp = { type: "surface", [field]: s.id, confirm_price: s.price };
                  if (s.owned) send(op);
                  else
                    setConfirm({
                      label: t("Buy {name} for {price} {currency}?", {
                        name: surfaceName(s),
                        price: s.price,
                        currency: t(s.currency.toLowerCase()),
                      }),
                      op,
                    });
                }}
              >
                {surfaces.data?.[key].map((s) => (
                  <option key={s.id} value={s.id}>
                    {surfaceName(s)} · {s.owned ? t("Owned") : `${s.price} ${t(s.currency.toLowerCase())}`}
                  </option>
                ))}
              </select>
            </label>
          ))}
          <PixelButton
            disabled={pending}
            variant="secondary"
            onClick={() => send({ type: "lock", locked: !house.house.locked })}
          >
            {house.house.locked ? t("Unlock house") : t("Lock house")}
          </PixelButton>
          {house.next && (
            <PixelButton
              disabled={pending}
              onClick={() =>
                setConfirm({
                  label: t("Upgrade to {name}: {width}×{depth}, {rooms} rooms for {price} shells?", {
                    ...house.next!,
                    name: tierLabel(house.house.tier + 1, house.next!.name),
                  }),
                  op: { type: "upgrade", from_tier: house.house.tier },
                })
              }
            >
              {t("Upgrade house")}
            </PixelButton>
          )}
          <PixelButton variant="secondary" onClick={() => setNotesOpen(!notesOpen)}>
            {t("Guestbook")}
          </PixelButton>
          {notesOpen && (
            <div>
              {notes.data?.notes.map((n) => (
                <blockquote key={n.id}>
                  <strong>{n.author_name}</strong>
                  <p>{n.body}</p>
                </blockquote>
              ))}
              {notes.data?.notes.length === 0 && <p>{t("Nothing yet.")}</p>}
            </div>
          )}
        </details>
      )}
    </>
  );
}
