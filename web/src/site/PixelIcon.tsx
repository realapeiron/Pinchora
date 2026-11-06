import { ICONS, ICON_SIZE, runs } from "./icons";
import type { IconName } from "./features";

/** A feature's icon, drawn pixel by pixel; `scale` is the width of one art pixel. */
export function PixelIcon({
  name,
  scale = 4,
  className,
}: {
  name: IconName;
  scale?: number;
  className?: string;
}) {
  return (
    <svg
      className={className}
      width={ICON_SIZE * scale}
      height={ICON_SIZE * scale}
      viewBox={`0 0 ${ICON_SIZE} ${ICON_SIZE}`}
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      {runs(ICONS[name]).map((r) => (
        <rect key={`${r.x}-${r.y}`} x={r.x} y={r.y} width={r.w} height={1} fill={r.fill} />
      ))}
    </svg>
  );
}
