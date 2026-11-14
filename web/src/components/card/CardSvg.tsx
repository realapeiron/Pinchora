/**
 * A design drawn by the browser, layer for layer as `pinchora-render` draws it, for the editor's canvas and the
 * template gallery. The picture the bot posts is the PNG preview next to it; this one answers every drag at once.
 */

import { translate as spaTranslate } from "../../i18n";
import { useId, type ReactNode } from "react";
import type { CardDesign, Fill, Layer } from "../../lib/card";
import type { AtlasIndex } from "./svg/atlas";
import { Background } from "./svg/Background";
import { Avatar } from "./svg/Avatar";
import { Text } from "./svg/Text";
import { Sticker } from "./svg/Sticker";
import { Shape } from "./svg/Shape";

export type { AtlasIndex } from "./svg/atlas";
export { ATLAS_URL, ATLAS_SIZE, GLYPHS } from "./svg/atlas";
export { notchedPath } from "./svg/Shape";
export { textWidth, wrapText } from "./svg/textMeasure";

/** The design as SVG, `width` px wide on the page (the drawing keeps the design's own coordinates). */
export function CardSvg({
  design,
  fill,
  avatarUrl,
  uploads,
  atlas,
  width,
  className,
}: {
  design: CardDesign;
  fill: Fill;
  avatarUrl: string;
  uploads: Record<string, string>;
  atlas: AtlasIndex | undefined;
  width?: number;
  className?: string;
}) {
  const uid = useId().replace(/:/g, "");
  const scale = width ? width / design.width : 1;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      width={design.width * scale}
      height={design.height * scale}
      viewBox={`0 0 ${design.width} ${design.height}`}
      role="img"
      aria-label={spaTranslate("The image as designed")}
    >
      <Background design={design} uid={uid} atlas={atlas} uploads={uploads} />
      {design.dim > 0 && (
        <rect width={design.width} height={design.height} fill="#000000" fillOpacity={design.dim / 100} />
      )}
      {design.layers.map((layer, i) =>
        layer.hidden || layer.opacity === 0 ? null : (
          <LayerArt
            key={layer.id}
            layer={layer}
            index={i}
            uid={uid}
            fill={fill}
            avatarUrl={avatarUrl}
            atlas={atlas}
          />
        ),
      )}
    </svg>
  );
}

function LayerArt({
  layer,
  index,
  uid,
  fill,
  avatarUrl,
  atlas,
}: {
  layer: Layer;
  index: number;
  uid: string;
  fill: Fill;
  avatarUrl: string;
  atlas: AtlasIndex | undefined;
}) {
  const alpha = Math.min(100, layer.opacity) / 100;
  const plainShape = layer.type === "shape" && layer.border_width === 0;
  let art: ReactNode;
  switch (layer.type) {
    case "avatar":
      art = <Avatar layer={layer} avatarUrl={avatarUrl} clip={`${uid}clip-${index}`} />;
      break;
    case "text":
      art = <Text layer={layer} fill={fill} />;
      break;
    case "sprite":
      art = <Sticker layer={layer} atlas={atlas} />;
      break;
    case "shape":
      art = <Shape layer={layer} fade={plainShape ? alpha : 1} />;
      break;
  }
  return alpha < 1 && !plainShape ? <g opacity={alpha}>{art}</g> : <>{art}</>;
}
