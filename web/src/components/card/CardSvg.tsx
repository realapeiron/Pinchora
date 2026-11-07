import { translate as spaTranslate } from "../../i18n";
import { type ReactNode, useId } from "react";

import {
  type AvatarLayer,
  type CardDesign,
  type Fill,
  type Layer,
  type ShapeLayer,
  type SpriteLayer,
  type TextLayer,
  CARD,
  coverScale,
  fillPlaceholders,
} from "../../lib/card";

/**
 * A design drawn by the browser, layer for layer as `pinchora-render` draws it, for the editor's canvas and the
 * template gallery. The picture the bot posts is the PNG preview next to it; this one answers every drag at once.
 */

/** Where each atlas sprite sits in the sheet (`/assets/atlas.json`). */
export type AtlasIndex = Record<string, { x: number; y: number; w: number; h: number }>;
export const ATLAS_URL = "/assets/atlas.png";
export const ATLAS_SIZE = { w: 1024, h: 9080 };

/** The little pixel drawings, as the bot has them: `#` the colour, `o` the light. */
export const GLYPHS: Record<string, string[]> = {
  crab: ["#.......#", "##.....##", ".#.o.o.#.", ".#######.", "#########", ".#.#.#.#."],
  heart: [".##.##.", "#o#####", "#######", ".#####.", "..###..", "...#..."],
  star: ["...#...", "..###..", "#######", ".#####.", ".##.##.", "#.....#"],
  shell: ["..###..", ".#o#o#.", "#o#o#o#", "#######", ".#.#.#.", "..###.."],
};
const GLYPH_LIGHT = "#fff1e8";
const AVATAR_BACK = "#0b1029";
const SPARKLES: [number, number][] = [
  [0.06, 0.12],
  [0.93, 0.08],
  [0.88, 0.9],
  [0.4, 0.05],
  [0.7, 0.94],
  [0.03, 0.85],
];

export function notchedPath(x: number, y: number, w: number, h: number, p: number): string {
  const [a, b, c, d] = [x + p, x + w - p, y + p, y + h - p];
  return `M${a},${y} H${b} V${c} H${x + w} V${d} H${b} V${y + h} H${a} V${d} H${x} V${c} H${a} Z`;
}

// /Text

let measurer: CanvasRenderingContext2D | null = null;

/** How wide `text` draws, as near as the browser can tell: the bot measures with the same font files. */
export function textWidth(text: string, family: string, bold: boolean, size: number): number {
  measurer ??= document.createElement("canvas").getContext("2d");
  if (!measurer) return text.length * size * 0.6;
  measurer.font = `${bold ? 700 : 400} ${size}px "${family}"`;
  return measurer.measureText(text).width;
}

/** The longest start of `text` that fits `max` with an ellipsis after it; `text` itself when it fits. */
function fit(text: string, max: number, width: (t: string) => number): string {
  if (width(text) <= max) return text;
  const graphemes = [...new Intl.Segmenter("en", { granularity: "grapheme" }).segment(text)].map(
    (s) => s.segment,
  );
  let [fits, over] = [0, graphemes.length];
  while (over - fits > 1) {
    const mid = (fits + over) >> 1;
    if (width(graphemes.slice(0, mid).join("") + "…") <= max) fits = mid;
    else over = mid;
  }
  return graphemes.slice(0, fits).join("") + "…";
}

/** `text` in at most `maxLines` lines of at most `max` px, as the bot wraps it. */
export function wrapText(
  text: string,
  max: number,
  maxLines: number,
  width: (t: string) => number,
): string[] {
  const words = text.split(/\s+/).filter(Boolean);
  if (maxLines <= 1) return [fit(words.join(" "), max, width)];
  const lines: string[] = [];
  const starts = [0];
  let line = "";
  words.forEach((word, i) => {
    if (!line) {
      line = word;
      return;
    }
    const candidate = `${line} ${word}`;
    if (width(candidate) <= max) line = candidate;
    else {
      lines.push(line);
      line = word;
      starts.push(i);
    }
  });
  if (line) lines.push(line);
  if (lines.length > maxLines) {
    lines.length = maxLines;
    lines[maxLines - 1] = words.slice(starts[maxLines - 1]).join(" ");
  }
  return lines.map((l) => fit(l, max, width));
}

// /Drawing

interface Ctx {
  uid: string;
  fill: Fill;
  avatarUrl: string;
  uploads: Record<string, string>;
  atlas: AtlasIndex | undefined;
}

/** A piece of the atlas as an SVG viewport: `region` of the sheet drawn into the box. */
function AtlasCut({
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

function Background({ design, ctx }: { design: CardDesign; ctx: Ctx }) {
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
            <linearGradient id={`${ctx.uid}bg`} x1="0" y1="0" x2={x2} y2={y2}>
              <stop offset="0" stopColor={bg.from} />
              <stop offset="1" stopColor={bg.to} />
            </linearGradient>
            <pattern id={`${ctx.uid}dither`} width="8" height="8" patternUnits="userSpaceOnUse">
              <rect width="4" height="4" fill="#ffffff" fillOpacity="0.025" />
              <rect x="4" y="4" width="4" height="4" fill="#ffffff" fillOpacity="0.025" />
            </pattern>
          </defs>
          <rect width={w} height={h} fill={`url(#${ctx.uid}bg)`} />
          {bg.pixel && (
            <>
              <rect width={w} height={h} fill={`url(#${ctx.uid}dither)`} />
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
      const region = ctx.atlas?.[bg.id];
      if (!region) return null;
      const cover = coverScale(region.w, region.h, w, h);
      return <AtlasCut region={region} x={cover.x} y={cover.y} w={cover.w} h={cover.h} />;
    }
    case "upload": {
      const url = ctx.uploads[bg.id];
      return url ? <image href={url} width={w} height={h} preserveAspectRatio="xMidYMid slice" /> : null;
    }
  }
}

function Avatar({ layer, ctx, clip }: { layer: AvatarLayer; ctx: Ctx; clip: string }) {
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
        href={ctx.avatarUrl}
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

function Text({ layer, ctx }: { layer: TextLayer; ctx: Ctx }) {
  const family = layer.font === "pixel" ? "Silkscreen" : "Noto Sans";
  let text = fillPlaceholders(layer.text, ctx.fill);
  if (layer.uppercase) text = text.toUpperCase();
  const lines = wrapText(text, layer.w, layer.max_lines, (t) => textWidth(t, family, layer.bold, layer.size));
  const [ax, anchor]: [number, "start" | "middle" | "end"] =
    layer.align === "left"
      ? [layer.x, "start"]
      : layer.align === "right"
        ? [layer.x + layer.w, "end"]
        : [layer.x + layer.w / 2, "middle"];
  const drop = Math.max(2, Math.round(layer.size / 16));
  const outline =
    layer.outline_width > 0
      ? {
          stroke: layer.outline_color,
          strokeWidth: layer.outline_width * 2,
          strokeLinejoin: (layer.font === "pixel" ? "miter" : "round") as "miter" | "round",
          paintOrder: "stroke",
        }
      : {};
  const common = {
    fontFamily: family,
    fontSize: layer.size,
    fontWeight: layer.bold ? 700 : 400,
    textAnchor: anchor,
    style: { whiteSpace: "pre" as const },
  };
  return (
    <>
      {lines.map((line, i) => {
        if (!line) return null;
        const base = layer.y + layer.size + i * layer.size * CARD.line_height;
        return (
          <g key={i}>
            {layer.shadow && (
              <text
                {...common}
                {...outline}
                strokeOpacity={outline.stroke ? 0.45 : undefined}
                x={ax + drop}
                y={base + drop}
                fill="#000000"
                fillOpacity={0.45}
              >
                {line}
              </text>
            )}
            <text {...common} {...outline} x={ax} y={base} fill={layer.color}>
              {line}
            </text>
          </g>
        );
      })}
    </>
  );
}

function Sticker({ layer, ctx }: { layer: SpriteLayer; ctx: Ctx }) {
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
  const region = ctx.atlas?.[layer.sprite];
  if (!region) return null;
  const scale = Math.min(w / region.w, h / region.h);
  const [dw, dh] = [region.w * scale, region.h * scale];
  return <AtlasCut region={region} x={x + (w - dw) / 2} y={y + (h - dh) / 2} w={dw} h={dh} />;
}

function Shape({ layer, fade }: { layer: ShapeLayer; fade: number }) {
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

function LayerArt({ layer, index, ctx }: { layer: Layer; index: number; ctx: Ctx }) {
  const alpha = Math.min(100, layer.opacity) / 100;
  const plainShape = layer.type === "shape" && layer.border_width === 0;
  let art: ReactNode;
  switch (layer.type) {
    case "avatar":
      art = <Avatar layer={layer} ctx={ctx} clip={`${ctx.uid}clip-${index}`} />;
      break;
    case "text":
      art = <Text layer={layer} ctx={ctx} />;
      break;
    case "sprite":
      art = <Sticker layer={layer} ctx={ctx} />;
      break;
    case "shape":
      art = <Shape layer={layer} fade={plainShape ? alpha : 1} />;
      break;
  }
  return alpha < 1 && !plainShape ? <g opacity={alpha}>{art}</g> : <>{art}</>;
}

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
  const ctx: Ctx = { uid, fill, avatarUrl, uploads, atlas };
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
      <Background design={design} ctx={ctx} />
      {design.dim > 0 && (
        <rect width={design.width} height={design.height} fill="#000000" fillOpacity={design.dim / 100} />
      )}
      {design.layers.map((layer, i) =>
        layer.hidden || layer.opacity === 0 ? null : (
          <LayerArt key={layer.id} layer={layer} index={i} ctx={ctx} />
        ),
      )}
    </svg>
  );
}
