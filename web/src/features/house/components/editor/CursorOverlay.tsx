import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import type { Art } from "../../../../iso/art";
import { anchor, renderRoom } from "../../../../iso/render";
import type { RoomJson, SceneItem } from "../../../../iso/scene";
import { canPlace, footprint, surfaceLift } from "../../../../iso/rules";
import { levelTop } from "../../../../iso/projection";
import { RoomCursor, type Piece } from "../../api/houseEdit";
import { translate as t } from "../../../../i18n";
import { PixelButton } from "../../../../components/PixelButton";

type CursorProps = {
  cursorStore: RoomCursor;
  art: Art;
  room: RoomJson;
  others: Piece[];
  held: string | null;
  flipped: boolean;
};
function useCursorPreview({ cursorStore, art, room, others, held, flipped }: CursorProps) {
  const cursor = useSyncExternalStore(cursorStore.subscribe, cursorStore.getSnapshot);
  const fp = held ? footprint(art.content, held, cursor[0], cursor[1], flipped) : null;
  const reason = held
    ? canPlace(art.content, room.width, room.depth, others, held, cursor[0], cursor[1], flipped)
    : null;
  return { cursor, fp, reason };
}
export function PlaceButton(props: CursorProps & { onPlace: () => void }) {
  const { reason } = useCursorPreview(props);
  return (
    <PixelButton onClick={props.onPlace} disabled={!props.held || !!reason}>
      {t("Place")}
    </PixelButton>
  );
}
export function CursorStatus(props: CursorProps & { message: string }) {
  const { cursor, reason } = useCursorPreview(props);
  return (
    <div className="house-info" role="status" aria-live="polite">
      {t((reason ?? props.message) || "Choose furniture, then click a tile. Alt + drag pans the room.")} ·{" "}
      {cursor[0] + 1},{cursor[1] + 1}
    </div>
  );
}
export function CursorPreview(
  props: CursorProps & {
    activeWall: "floor" | "left" | "right";
    drawn: ReturnType<typeof renderRoom>;
    ox: number;
    oy: number;
  },
) {
  const { art, others, held, flipped, activeWall, drawn, ox, oy } = props;
  const { cursor, fp, reason } = useCursorPreview(props);
  const overlay = useRef<HTMLCanvasElement>(null);
  const ghostSprite = useMemo(() => {
    const sprite = held ? art.item(held) : null;
    if (!sprite) return null;
    const canvas = document.createElement("canvas");
    canvas.width = sprite.width;
    canvas.height = sprite.height;
    canvas.getContext("2d")?.putImageData(new ImageData(sprite.data, sprite.width, sprite.height), 0, 0);
    return { sprite, canvas };
  }, [art, held]);
  useEffect(() => {
    const ctx = overlay.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, drawn.image.width, drawn.image.height);
    const [x, y] = cursor;
    const w = fp?.rect.w ?? 1,
      d = fp?.rect.d ?? 1;
    const points =
      activeWall === "floor"
        ? [
            [32 * (x - y), 16 * (x + y)],
            [32 * (x + w - y), 16 * (x + w + y)],
            [32 * (x + w - y - d), 16 * (x + w + y + d)],
            [32 * (x - y - d), 16 * (x + y + d)],
          ]
        : (() => {
            const left = activeWall === "left";
            const a = (left ? -1 : 1) * 32 * x,
              b = (left ? -1 : 1) * 32 * (x + w);
            return [
              [a, levelTop(a) + 32 * y],
              [b, levelTop(b) + 32 * y],
              [b, levelTop(b) + 32 * (y + d)],
              [a, levelTop(a) + 32 * (y + d)],
            ];
          })();
    ctx.beginPath();
    points.forEach(([px, py], i) => (i ? ctx.lineTo(px! + ox, py! + oy) : ctx.moveTo(px! + ox, py! + oy)));
    ctx.closePath();
    ctx.fillStyle = reason ? "#ff4d6d55" : "#06d6a055";
    ctx.fill();
    ctx.strokeStyle = reason ? "#ff4d6d" : "#06d6a0";
    ctx.lineWidth = 3;
    ctx.stroke();
    if (ghostSprite && fp) {
      const { sprite, canvas: tmp } = ghostSprite;
      const item = { x, y, w, d, layer: fp.layer, lift: surfaceLift(art.content, others, x, y) } as SceneItem;
      const [sx, sy] = anchor(item, sprite);
      ctx.globalAlpha = 0.55;
      ctx.imageSmoothingEnabled = false;
      if (fp.layer === "wall") {
        ctx.save();
        const sign = flipped ? -1 : 1;
        ctx.transform(sign, 0.5, 0, 1, ox + sign * x * 32, oy + levelTop(sign * x * 32) + y * 32);
        ctx.drawImage(tmp, 0, 0, w * 32, d * 32);
        ctx.restore();
      } else {
        ctx.save();
        if (flipped) {
          ctx.translate(sx + ox + sprite.width, sy + oy);
          ctx.scale(-1, 1);
          ctx.drawImage(tmp, 0, 0);
        } else ctx.drawImage(tmp, sx + ox, sy + oy);
        ctx.restore();
      }
      ctx.globalAlpha = 1;
    }
  }, [cursor, flipped, held, fp, reason, activeWall, art, drawn, ox, oy, others, ghostSprite]);
  return (
    <canvas
      ref={overlay}
      className="room-canvas room-ghost"
      width={drawn.image.width}
      height={drawn.image.height}
      aria-hidden="true"
    />
  );
}
