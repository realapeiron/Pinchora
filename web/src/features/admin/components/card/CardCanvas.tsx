import { type PointerEvent as ReactPointerEvent, useRef, useState } from "react";

import {
  type Box,
  type CardDesign,
  type Fill,
  type Guide,
  type Handle,
  type Keep,
  type Layer,
  HANDLES,
  fitLayer,
  maxAvatarSide,
  resizeBox,
  resizeText,
  snapMove,
} from "../../../../lib/card";
import { type AtlasIndex, CardSvg, GLYPHS } from "../../../../components/card/CardSvg";

/** How close, in screen pixels, a moving layer snaps to a guide. */
const SNAP_PX = 6;

/** How a layer may be resized: avatars stay square, stickers keep their shape (atlas ones in whole pixels). */
function keepOf(layer: Layer, atlas: AtlasIndex | undefined): Keep {
  if (layer.type === "avatar") return { kind: "square" };
  if (layer.type === "sprite") {
    const region = atlas?.[layer.sprite];
    if (region) return { kind: "aspect", unit: { w: region.w, h: region.h } };
    const rows = GLYPHS[layer.sprite];
    return rows
      ? { kind: "aspect", unit: { w: Math.max(...rows.map((r) => r.length)), h: rows.length } }
      : { kind: "aspect" };
  }
  return { kind: "free" };
}

interface Drag {
  pointer: number;
  id: string;
  /** `move`, or the handle being pulled. */
  what: "move" | Handle;
  from: { x: number; y: number };
  start: Layer;
}

/**
 * The editing canvas: the design drawn by the browser at `zoom`, with a hit box over every layer, handles on
 * the selected one, and guides while a layer snaps. Pointer events, so a mouse, a pen and a finger all work;
 * Alt (or the snapping switch) turns snapping off.
 */
export function CardCanvas({
  design,
  zoom,
  selected,
  snapping,
  safeArea,
  fill,
  avatarUrl,
  uploads,
  atlas,
  onSelect,
  onLive,
  onCommit,
}: {
  design: CardDesign;
  zoom: number;
  selected: string | null;
  snapping: boolean;
  safeArea?: boolean;
  fill: Fill;
  avatarUrl: string;
  uploads: Record<string, string>;
  atlas: AtlasIndex | undefined;
  onSelect: (id: string | null) => void;
  /** The design while a drag goes on. */
  onLive: (design: CardDesign) => void;
  /** A drag ended: whatever it changed is one undo step. */
  onCommit: () => void;
}) {
  const drag = useRef<Drag | null>(null);
  const [guides, setGuides] = useState<Guide[]>([]);
  const current = design.layers.find((l) => l.id === selected && !l.hidden);

  const begin = (event: ReactPointerEvent, layer: Layer, what: Drag["what"]) => {
    if (event.button !== 0) return;
    event.stopPropagation();
    onSelect(layer.id);
    if (layer.locked) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag.current = {
      pointer: event.pointerId,
      id: layer.id,
      what,
      from: { x: event.clientX, y: event.clientY },
      start: layer,
    };
  };

  const move = (event: ReactPointerEvent) => {
    const d = drag.current;
    if (!d || d.pointer !== event.pointerId) return;
    const dx = (event.clientX - d.from.x) / zoom;
    const dy = (event.clientY - d.from.y) / zoom;
    let next: Layer;
    let lines: Guide[] = [];
    if (d.what === "move") {
      const moved: Box = { ...d.start, x: Math.round(d.start.x + dx), y: Math.round(d.start.y + dy) };
      if (snapping && !event.altKey) {
        const others = design.layers.filter((l) => l.id !== d.id && !l.hidden);
        const snapped = snapMove(moved, others, design, SNAP_PX / zoom);
        lines = snapped.guides;
        next = { ...d.start, x: snapped.box.x, y: snapped.box.y };
      } else {
        next = { ...d.start, x: moved.x, y: moved.y };
      }
    } else if (d.start.type === "text") {
      next = resizeText(d.start, d.what, dx, dy, design);
    } else {
      // Never bigger than the canvas (an avatar also within its own cap), as the API wants it.
      const side = maxAvatarSide(design);
      const max = d.start.type === "avatar" ? { w: side, h: side } : { w: design.width, h: design.height };
      next = { ...d.start, ...resizeBox(d.start, d.what, dx, dy, keepOf(d.start, atlas), 8, max) };
    }
    setGuides(lines);
    onLive({ ...design, layers: design.layers.map((l) => (l.id === d.id ? fitLayer(next, design) : l)) });
  };

  const end = (event: ReactPointerEvent) => {
    const d = drag.current;
    if (!d || d.pointer !== event.pointerId) return;
    drag.current = null;
    setGuides([]);
    onCommit();
  };

  const at = (box: Box) => ({
    left: box.x * zoom,
    top: box.y * zoom,
    width: Math.max(box.w * zoom, 4),
    height: Math.max(box.h * zoom, 4),
  });

  return (
    <div
      className={`card-stage ${design.background.type === "transparent" ? "is-transparent" : ""}`}
      style={{ width: design.width * zoom, height: design.height * zoom }}
      onPointerDown={() => onSelect(null)}
      data-testid="card-stage"
    >
      <CardSvg
        design={design}
        fill={fill}
        avatarUrl={avatarUrl}
        uploads={uploads}
        atlas={atlas}
        width={design.width * zoom}
        className="card-svg"
      />
      <div className="card-overlay">
        {safeArea && (
          <div
            className="card-safe-area"
            aria-hidden="true"
            style={{
              inset: `${Math.round(design.height * zoom * 0.07)}px ${Math.round(design.width * zoom * 0.07)}px`,
            }}
          />
        )}
        {design.layers.map((layer) =>
          layer.hidden ? null : (
            <div
              key={layer.id}
              className={`card-hit ${layer.locked ? "is-locked" : ""} ${layer.id === selected ? "is-selected" : ""}`}
              style={at(layer)}
              data-layer={layer.id}
              onPointerDown={(event) => begin(event, layer, "move")}
              onPointerMove={move}
              onPointerUp={end}
              onPointerCancel={end}
            />
          ),
        )}
        {current && (
          <div className={`card-selection ${current.locked ? "is-locked" : ""}`} style={at(current)}>
            {!current.locked &&
              HANDLES.filter((h) => current.type !== "avatar" || h.length === 2).map((handle) => (
                <span
                  key={handle}
                  className={`card-handle card-handle-${handle}`}
                  data-handle={handle}
                  onPointerDown={(event) => begin(event, current, handle)}
                  onPointerMove={move}
                  onPointerUp={end}
                  onPointerCancel={end}
                />
              ))}
          </div>
        )}
        {guides.map((g) => (
          <span
            key={`${g.axis}${g.at}`}
            className={`card-guide card-guide-${g.axis}`}
            style={g.axis === "x" ? { left: g.at * zoom } : { top: g.at * zoom }}
          />
        ))}
      </div>
    </div>
  );
}
