import type { ReactNode } from "react";
import type { AvatarLayer } from "../../../lib/card";
import { notchedPath } from "./Shape";

const AVATAR_BACK = "#0b1029";

export function Avatar({ layer, avatarUrl, clip }: { layer: AvatarLayer; avatarUrl: string; clip: string }) {
  const { x, y, w: size, border_width: b, border_color: edge } = layer;
  const inner = Math.max(1, size - 2 * b);
  const [ix, iy] = [x + b, y + b];
  let frame: ReactNode = null;
  let clipShape: ReactNode;
  switch (layer.shape) {
    case "notched": {
      const p = b > 0 ? b : Math.round(Math.max(2, size / 16));
      if (b > 0)
        frame = (
          <>
            <path d={notchedPath(x, y, size, size, p)} fill={edge} />
            <path d={notchedPath(ix, iy, inner, inner, p)} fill={AVATAR_BACK} />
          </>
        );
      clipShape = <path d={notchedPath(ix, iy, inner, inner, p)} />;
      break;
    }
    case "circle": {
      const [cx, cy] = [x + size / 2, y + size / 2];
      if (b > 0)
        frame = (
          <>
            <circle cx={cx} cy={cy} r={size / 2} fill={edge} />
            <circle cx={cx} cy={cy} r={inner / 2} fill={AVATAR_BACK} />
          </>
        );
      clipShape = <circle cx={cx} cy={cy} r={inner / 2} />;
      break;
    }
    default: {
      const r = layer.shape === "rounded" ? size * 0.16 : 0;
      const ir = Math.max(0, r - b);
      if (b > 0)
        frame = (
          <>
            <rect x={x} y={y} width={size} height={size} rx={r} fill={edge} />
            <rect x={ix} y={iy} width={inner} height={inner} rx={ir} fill={AVATAR_BACK} />
          </>
        );
      clipShape = <rect x={ix} y={iy} width={inner} height={inner} rx={ir} />;
    }
  }
  return (
    <>
      {frame}
      <clipPath id={clip}>{clipShape}</clipPath>
      <image
        href={avatarUrl}
        x={ix}
        y={iy}
        width={inner}
        height={inner}
        preserveAspectRatio="none"
        clipPath={`url(#${clip})`}
        style={{ imageRendering: layer.pixelate ? "pixelated" : "auto" }}
      />
    </>
  );
}
