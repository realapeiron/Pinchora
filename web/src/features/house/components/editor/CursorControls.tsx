import { translate as t } from "../../../../i18n";
import type { HouseEditorModel } from "./useHouseEditor";
import { PixelButton } from "../../../../components/PixelButton";
import { CursorStatus } from "./CursorOverlay";

export function CursorControls({
  model,
}: {
  model: Pick<
    HouseEditorModel,
    | "cursorStore"
    | "art"
    | "room"
    | "others"
    | "held"
    | "flipped"
    | "message"
    | "activeWall"
    | "setWall"
    | "selected"
    | "setSelected"
    | "setHand"
    | "setMoving"
    | "target"
    | "pending"
    | "setFlipped"
  >;
}) {
  const {
    cursorStore,
    art,
    room,
    others,
    held,
    flipped,
    message,
    activeWall,
    setWall,
    selected,
    setSelected,
    setHand,
    setMoving,
    target,
    pending,
    setFlipped,
  } = model;
  return (
    <>
      <CursorStatus
        cursorStore={cursorStore}
        art={art}
        room={room}
        others={others}
        held={held}
        flipped={flipped}
        message={message}
      />
      <div className="house-options">
        <label>
          {t("Cursor plane")}
          <select
            value={activeWall}
            disabled={!!held}
            onChange={(e) => setWall(e.target.value as HouseEditorModel["wall"])}
          >
            <option value="floor">{t("Floor")}</option>
            <option value="right">{t("Right wall")}</option>
            <option value="left">{t("Left wall")}</option>
          </select>
        </label>
        {(
          [
            [-1, 0, "Move cursor left"],
            [1, 0, "Move cursor right"],
            [0, -1, "Move cursor up"],
            [0, 1, "Move cursor down"],
          ] as const
        ).map(([dx, dy, label]) => (
          <PixelButton
            key={label}
            variant="secondary"
            onClick={() => {
              const p: [number, number] = [
                Math.max(0, cursorStore.get()[0] + dx),
                Math.max(0, cursorStore.get()[1] + dy),
              ];
              cursorStore.move(p[0], p[1]);
            }}
          >
            {t(label)}
          </PixelButton>
        ))}
      </div>
      <label>
        {t("Select a placed piece")}
        <select
          value={selected ?? ""}
          onChange={(e) => {
            setSelected(e.target.value || null);
            setHand(null);
            setMoving(null);
          }}
        >
          <option value="">{t("Choose a piece")}</option>
          {room.placements.map((p) => (
            <option key={p.id} value={p.id}>
              {art.content.itemName(art.content.item(p.item_id)!)} · {p.x + 1},{p.y + 1}
            </option>
          ))}
        </select>
      </label>
      {selected && target.kind !== "partner" && (
        <PixelButton
          disabled={pending}
          variant="secondary"
          onClick={() => {
            const p = room.placements.find((p) => p.id === selected);
            if (p) {
              setMoving(p);
              setFlipped(p.flipped);
            }
          }}
        >
          {t("Move selected piece")}
        </PixelButton>
      )}
    </>
  );
}
