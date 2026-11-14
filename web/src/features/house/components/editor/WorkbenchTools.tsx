import { translate as t } from "../../../../i18n";
import type { HouseEditorModel } from "./useHouseEditor";
import { PixelButton } from "../../../../components/PixelButton";
import { PlaceButton } from "./CursorOverlay";

export function WorkbenchTools({
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
    | "place"
    | "turn"
    | "pickup"
    | "undo"
    | "pending"
    | "selected"
    | "reapply"
    | "redo"
    | "setConfirm"
    | "onExit"
  >;
}) {
  const {
    cursorStore,
    art,
    room,
    others,
    held,
    flipped,
    place,
    turn,
    pickup,
    undo,
    pending,
    selected,
    reapply,
    redo,
    setConfirm,
    onExit,
  } = model;
  return (
    <div className="house-options" role="toolbar" aria-label={t("Editor tools")}>
      <PlaceButton
        cursorStore={cursorStore}
        art={art}
        room={room}
        others={others}
        held={held}
        flipped={flipped}
        onPlace={place}
      />
      <PixelButton onClick={turn} disabled={!held && (!selected || pending)}>
        {t("Turn / flip")}
      </PixelButton>
      <PixelButton onClick={pickup} disabled={!selected || pending}>
        {t("Store")}
      </PixelButton>
      <PixelButton onClick={undo} disabled={pending || !room.undo?.length}>
        {t("Undo")}
      </PixelButton>
      <PixelButton
        onClick={reapply}
        disabled={pending || !redo.length}
        title={t("Redo re-applies the operation.")}
      >
        {t("Redo")}
      </PixelButton>
      <PixelButton
        variant="secondary"
        disabled={pending || !room.placements.length}
        onClick={() => setConfirm({ label: t("Store every piece in this room?"), op: { type: "clear" } })}
      >
        {t("Clear room")}
      </PixelButton>
      <PixelButton variant="ghost" disabled={pending} onClick={onExit}>
        {t("View mode")}
      </PixelButton>
    </div>
  );
}
