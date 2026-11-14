import { useMemo } from "react";
import type { Art } from "../../../../iso/art";
import { bounds, renderRoom } from "../../../../iso/render";
import { buildScene, type RoomJson } from "../../../../iso/scene";
import { contains, footprint } from "../../../../iso/rules";
import { MARGIN } from "../../../../iso/projection";
import type { Piece } from "../../api/houseEdit";

export function useEditorScene(
  art: Art,
  room: RoomJson,
  selected: string | null,
  moving: Piece | null,
  hand: string | null,
  flipped: boolean,
  wall: "floor" | "right" | "left",
) {
  const completeScene = useMemo(
    () => buildScene(art.content, art, room, { edit: true, selected }),
    [art, room, selected],
  );
  const liftedIds = useMemo(() => {
    const ids = new Set<string>();
    if (!moving) return ids;
    ids.add(moving.id);
    const base = footprint(art.content, moving.item_id, moving.x, moving.y, moving.flipped);
    if (base?.layer === "floor")
      for (const p of room.placements) {
        const top = footprint(art.content, p.item_id, p.x, p.y, p.flipped);
        if (top?.layer === "surface" && contains(base.rect, top.rect)) ids.add(p.id);
      }
    return ids;
  }, [art, moving, room.placements]);
  const scene = useMemo(
    () => ({
      ...completeScene,
      items: completeScene.items.filter((i) => !i.info.placement || !liftedIds.has(i.info.placement)),
    }),
    [completeScene, liftedIds],
  );
  const box = useMemo(
    () =>
      bounds(
        completeScene,
        completeScene.items.map((i) => i.sprite),
      ),
    [completeScene],
  );
  const drawn = useMemo(() => renderRoom(scene, "all", box), [scene, box]);
  const ox = MARGIN - box[0],
    oy = MARGIN - box[1];
  const held = moving?.item_id ?? hand;
  const others = useMemo(
    () => room.placements.filter((p) => !liftedIds.has(p.id)),
    [room.placements, liftedIds],
  );
  const activeWall =
    held && art.content.item(held)?.base.layer === "wall"
      ? flipped
        ? "left"
        : "right"
      : held
        ? "floor"
        : wall;
  return { scene, drawn, ox, oy, held, others, activeWall };
}
