import { translate as t } from "../../../../i18n";
import type { HouseEditorModel } from "./useHouseEditor";
import { CursorPreview } from "./CursorOverlay";
import { useState, type ComponentProps } from "react";
import { RoomStage } from "../RoomStage";

export function RoomInteraction({
  model,
}: {
  model: Pick<
    HouseEditorModel,
    | "drawn"
    | "cursorStore"
    | "art"
    | "room"
    | "others"
    | "held"
    | "flipped"
    | "activeWall"
    | "ox"
    | "oy"
    | "point"
    | "scene"
    | "gesture"
    | "hand"
    | "setSelected"
    | "setFlipped"
    | "target"
    | "setMoving"
    | "place"
    | "send"
  >;
}) {
  const {
    drawn,
    cursorStore,
    art,
    room,
    others,
    held,
    flipped,
    activeWall,
    ox,
    oy,
    point,
    scene,
    gesture,
    hand,
    setSelected,
    setFlipped,
    target,
    setMoving,
    place,
    send,
  } = model;
  return (
    <EditorStage
      rendered={drawn}
      label={t("Editable room")}
      editor={{
        overlay: (
          <CursorPreview
            cursorStore={cursorStore}
            art={art}
            room={room}
            others={others}
            held={held}
            flipped={flipped}
            activeWall={activeWall}
            drawn={drawn}
            ox={ox}
            oy={oy}
          />
        ),
        point,
        down: (index) => {
          const id = index === null ? null : scene.items[index]?.info.placement;
          const item = room.placements.find((p) => p.id === id) ?? null;
          gesture.current = { item, start: cursorStore.get() };
          if (!hand && item) {
            setSelected(item.id);
            setFlipped(item.flipped);
            if (target.kind !== "partner") setMoving(item);
          }
        },
        up: () => {
          if (target.kind === "partner" && !hand) {
            gesture.current = null;
            return;
          }
          const g = gesture.current;
          gesture.current = null;
          if (held) {
            if (
              g?.item &&
              !hand &&
              g.start[0] === cursorStore.get()[0] &&
              g.start[1] === cursorStore.get()[1]
            ) {
              setMoving(null);
              return;
            }
            place();
            return;
          }
          if (g?.item) {
            setSelected(g.item.id);
            if (g.start[0] !== cursorStore.get()[0] || g.start[1] !== cursorStore.get()[1]) {
              send({
                type: "move",
                id: g.item.id,
                x: cursorStore.get()[0],
                y: cursorStore.get()[1],
                flipped: g.item.flipped,
              });
              setMoving(null);
            } else setMoving(g.item);
          }
        },
        cancel: () => {
          gesture.current = null;
          setMoving(null);
        },
      }}
    />
  );
}

// Pointer hover only redraws the stage outline, not the inventory and editor controls.
function EditorStage(props: Omit<ComponentProps<typeof RoomStage>, "hover" | "onHover">) {
  const [hover, setHover] = useState<number | null>(null);
  return <RoomStage {...props} hover={hover} onHover={setHover} />;
}
