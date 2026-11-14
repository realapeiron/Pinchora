import { coverScale, type CardDesign } from "../../../lib/card";
import { AtlasCut } from "./AtlasCut";
import type { AtlasIndex } from "./atlas";

const SPARKLES: [number, number][] = [
  [0.06, 0.12],
  [0.93, 0.08],
  [0.88, 0.9],
  [0.4, 0.05],
  [0.7, 0.94],
  [0.03, 0.85],
];

export function Background({
  design,
  uid,
  atlas,
  uploads,
}: {
  design: CardDesign;
  uid: string;
  atlas: AtlasIndex | undefined;
  uploads: Record<string, string>;
}) {
  const { width: w, height: h, background: bg } = design;
  switch (bg.type) {
    case "transparent":
      return null;
    case "solid":
      return <rect width={w} height={h} fill={bg.color} />;
    case "gradient": {
      const [x2, y2] = { vertical: [0, 1], horizontal: [1, 0], diagonal: [1, 1] }[bg.direction];
      return (
        <>
          <defs>
            <linearGradient id={`${uid}bg`} x1="0" y1="0" x2={x2} y2={y2}>
              <stop offset="0" stopColor={bg.from} />
              <stop offset="1" stopColor={bg.to} />
            </linearGradient>
            <pattern id={`${uid}dither`} width="8" height="8" patternUnits="userSpaceOnUse">
              <rect width="4" height="4" fill="#ffffff" fillOpacity="0.025" />
              <rect x="4" y="4" width="4" height="4" fill="#ffffff" fillOpacity="0.025" />
            </pattern>
          </defs>
          <rect width={w} height={h} fill={`url(#${uid}bg)`} />
          {bg.pixel && (
            <>
              <rect width={w} height={h} fill={`url(#${uid}dither)`} />
              {SPARKLES.map(([sx, sy]) => (
                <rect
                  key={`${sx}-${sy}`}
                  x={Math.round(w * sx)}
                  y={Math.round(h * sy)}
                  width="4"
                  height="4"
                  fill="#ffffff"
                  fillOpacity="0.35"
                />
              ))}
            </>
          )}
        </>
      );
    }
    case "sprite": {
      const region = atlas?.[bg.id];
      if (!region) return null;
      const cover = coverScale(region.w, region.h, w, h);
      return <AtlasCut region={region} x={cover.x} y={cover.y} w={cover.w} h={cover.h} />;
    }
    case "upload": {
      const url = uploads[bg.id];
      return url ? <image href={url} width={w} height={h} preserveAspectRatio="xMidYMid slice" /> : null;
    }
  }
}
