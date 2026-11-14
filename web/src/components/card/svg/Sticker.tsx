import type { SpriteLayer } from "../../../lib/card";
import { AtlasCut } from "./AtlasCut";
import { GLYPHS, type AtlasIndex } from "./atlas";

const GLYPH_LIGHT = "#fff1e8";

export function Sticker({ layer, atlas }: { layer: SpriteLayer; atlas: AtlasIndex | undefined }) {
  const rows = GLYPHS[layer.sprite];
  const { x, y, w, h } = layer;
  if (rows) {
    const cols = Math.max(...rows.map((r) => r.length));
    const scale = Math.min(w / cols, h / rows.length);
    const [gx, gy] = [x + (w - cols * scale) / 2, y + (h - rows.length * scale) / 2];
    return (
      <>
        {rows.flatMap((row, r) =>
          [...row].map((ch, c) =>
            ch === "#" || ch === "o" ? (
              <rect
                key={`${r}-${c}`}
                x={gx + c * scale}
                y={gy + r * scale}
                width={scale}
                height={scale}
                fill={ch === "#" ? layer.color : GLYPH_LIGHT}
                shapeRendering="crispEdges"
              />
            ) : null,
          ),
        )}
      </>
    );
  }
  const region = atlas?.[layer.sprite];
  if (!region) return null;
  const scale = Math.min(w / region.w, h / region.h);
  const [dw, dh] = [region.w * scale, region.h * scale];
  return <AtlasCut region={region} x={x + (w - dw) / 2} y={y + (h - dh) / 2} w={dw} h={dh} />;
}
