import { ATLAS_SIZE, ATLAS_URL } from "./atlas";
/** A piece of the atlas as an SVG viewport: `region` of the sheet drawn into the box. */
export function AtlasCut({
  region,
  x,
  y,
  w,
  h,
}: {
  region: { x: number; y: number; w: number; h: number };
  x: number;
  y: number;
  w: number;
  h: number;
}) {
  return (
    <svg
      x={x}
      y={y}
      width={w}
      height={h}
      viewBox={`${region.x} ${region.y} ${region.w} ${region.h}`}
      preserveAspectRatio="none"
      overflow="hidden"
    >
      <image
        href={ATLAS_URL}
        width={ATLAS_SIZE.w}
        height={ATLAS_SIZE.h}
        style={{ imageRendering: "pixelated" }}
      />
    </svg>
  );
}
