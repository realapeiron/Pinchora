import { CARD, fillPlaceholders, type Fill, type TextLayer } from "../../../lib/card";
import { textWidth, wrapText } from "./textMeasure";

export function Text({ layer, fill }: { layer: TextLayer; fill: Fill }) {
  const family = layer.font === "pixel" ? "Silkscreen" : "Noto Sans";
  let text = fillPlaceholders(layer.text, fill);
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
