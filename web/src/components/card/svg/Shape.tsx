import type { ReactNode } from "react";
import type { ShapeLayer } from "../../../lib/card";

export function notchedPath(x: number, y: number, w: number, h: number, p: number): string {
  const [a, b, c, d] = [x + p, x + w - p, y + p, y + h - p];
  return `M${a},${y} H${b} V${c} H${x + w} V${d} H${b} V${y + h} H${a} V${d} H${x} V${c} H${a} Z`;
}
export function Shape({ layer, fade }: { layer: ShapeLayer; fade: number }) {
  const { x, y, w, h, fill, border_width: b, border_color: edge } = layer;
  const paint = { fill, fillOpacity: fade < 1 ? fade : undefined };
  const stroke = b > 0 ? { stroke: edge, strokeWidth: b } : {};
  const [bx, by, bw, bh] = [x + b / 2, y + b / 2, Math.max(0, w - b), Math.max(0, h - b)];
  switch (layer.shape) {
    case "rect":
      return <rect x={bx} y={by} width={bw} height={bh} {...paint} {...stroke} />;
    case "rounded":
      return (
        <rect
          x={bx}
          y={by}
          width={bw}
          height={bh}
          rx={Math.max(0, Math.min(w, h) * 0.2 - b / 2)}
          {...paint}
          {...stroke}
        />
      );
    case "ellipse":
      return <ellipse cx={x + w / 2} cy={y + h / 2} rx={bw / 2} ry={bh / 2} {...paint} {...stroke} />;
    case "notched": {
      const p = Math.max(b, Math.min(16, Math.max(2, Math.round(Math.min(w, h) / 12))));
      return (
        <>
          {b > 0 && <path d={notchedPath(x, y, w, h, p)} fill={edge} />}
          <path d={notchedPath(x + b, y + b, Math.max(0, w - 2 * b), Math.max(0, h - 2 * b), p)} {...paint} />
        </>
      );
    }
    case "dashes": {
      const dash = h * 2.5;
      const blocks: ReactNode[] = [];
      for (let bx2 = x; bx2 < x + w && dash > 0; bx2 += dash * 2) {
        blocks.push(
          <rect key={bx2} x={bx2} y={y} width={Math.min(dash, x + w - bx2)} height={h} {...paint} />,
        );
      }
      return <>{blocks}</>;
    }
  }
}
