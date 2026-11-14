import type { RoomJson } from "../iso/scene";
import type { HouseTarget } from "../features/house/api/house";
import { t } from "./i18n";
import type { Point } from "./manifest";
import { InventorySelection } from "./decorate/InventorySelection";
import { SurfaceSelection } from "./decorate/SurfaceSelection";
import { useDecorateRoom } from "./decorate/useDecorateRoom";
export interface Ghost {
  item_id: string;
  x: number;
  y: number;
  flipped: boolean;
  valid: boolean;
}
export interface EditBridge {
  ghost: Ghost | null;
  pick: (tile: Point | null, placement: string | null, hover: boolean) => void;
  key: (key: string) => boolean;
}
export function DecoratePanel({
  room,
  target,
  bridge,
  onSaved,
  onClose,
}: {
  room: RoomJson;
  target: HouseTarget;
  bridge: { current: EditBridge | null };
  onSaved: () => void;
  onClose: () => void;
}) {
  const {
    confirmed,
    pending,
    message,
    hand,
    setHand,
    selected,
    setSelected,
    tile,
    setTile,
    flipped,
    setFlipped,
    surface,
    setSurface,
    catalog,
    inventory,
    surfaces,
    piece,
    item,
    reason,
    changed,
    send,
    place,
  } = useDecorateRoom(room, target, bridge, onSaved);
  return (
    <aside className="reef-space-sheet reef-decorate" aria-label={t("Decorate")}>
      <div className="reef-drawer-heading">
        <h2>{t("Decorate")}</h2>
        <button type="button" onClick={onClose} disabled={pending}>
          {t("Done")}
        </button>
      </div>
      <p>{t("Choose a piece, position it, then confirm. R rotates; Enter places; Escape cancels.")}</p>
      <InventorySelection
        hand={hand}
        setHand={setHand}
        selected={selected}
        setSelected={setSelected}
        setTile={setTile}
        setFlipped={setFlipped}
        pending={pending}
        inventory={inventory.data}
        catalog={catalog.data}
        confirmed={confirmed}
      />
      <div className="reef-space-actions">
        <label>
          {t("Position X")}
          <input
            type="number"
            min={0}
            max={room.width - 1}
            value={tile[0]}
            onChange={(e) => setTile([Number(e.target.value), tile[1]])}
          />
        </label>
        <label>
          {t("Position Y / wall level")}
          <input
            type="number"
            min={0}
            max={room.depth - 1}
            value={tile[1]}
            onChange={(e) => setTile([tile[0], Number(e.target.value)])}
          />
        </label>
      </div>
      <p role="status">{reason ? t(reason) : item ? t("Ready to place") : t("Choose an item")}</p>
      <div className="reef-space-actions">
        <button
          type="button"
          disabled={!item || pending}
          aria-pressed={flipped}
          onClick={() => setFlipped(!flipped)}
        >
          {t("Rotate")}
        </button>
        <button type="button" disabled={!item || !!reason || pending || !changed} onClick={place}>
          {t("Confirm placement")}
        </button>
        <button
          type="button"
          disabled={!piece || pending}
          onClick={() => {
            send({ type: "pickup", id: selected });
            setSelected("");
          }}
        >
          {t("Store")}
        </button>
        <button
          type="button"
          disabled={!confirmed.undo?.length || pending}
          onClick={() => send({ type: "undo" })}
        >
          {t("Undo")}
        </button>
      </div>
      {(target.kind === "own" || target.kind === "plaza") && (
        <SurfaceSelection
          room={room}
          surface={surface}
          setSurface={setSurface}
          surfaces={surfaces.data}
          pending={pending}
          send={send}
        />
      )}
      <details>
        <summary>{t("Clear room")}</summary>
        <p>{t("Store every piece in this room?")}</p>
        <button
          type="button"
          disabled={pending || !confirmed.placements.length}
          onClick={() => send({ type: "clear" })}
        >
          {t("Confirm clear")}
        </button>
      </details>
      <p role="status">{t(message)}</p>
      {inventory.isError && <p role="alert">{t("This request could not be completed.")}</p>}
    </aside>
  );
}
