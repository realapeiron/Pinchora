import { translate as spaTranslate } from "../../../i18n";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import type { Rendered } from "../../../iso/render";
import { PixelButton } from "../../../components/PixelButton";

const MIN_ZOOM = 1;
const MAX_ZOOM = 6;

export interface Hover {
  /** Index in the scene's items of what the pointer is over. */
  item: number;
}

/**
 * A rendered room on a canvas at its native size, shown at a whole-number zoom (never blurred), with
 * drag to pan, Ctrl + wheel to zoom, and the piece under the pointer outlined and reported.
 */
export function RoomStage({
  rendered,
  hover,
  onHover,
  label,
  botPicture = null,
  editor,
}: {
  rendered: Rendered;
  hover: number | null;
  onHover: (item: number | null) => void;
  label: string;
  /** The bot's own picture of the room, laid over this one while comparing. */
  botPicture?: string | null;
  editor?: {
    overlay: ReactNode;
    point: (x: number, y: number) => void;
    down: (item: number | null) => void;
    up: () => void;
    cancel: () => void;
  };
}) {
  const { image, hit } = rendered;
  const [zoom, setZoom] = useState(MIN_ZOOM);
  const dpr = Math.max(1, window.devicePixelRatio || 1);
  const pixelRatio = Math.round(dpr) / dpr;
  const scale = zoom * pixelRatio;
  const stage = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const outline = useRef<HTMLCanvasElement>(null);
  const centre = useRef<[number, number] | null>(null);
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const fitted = useRef(false);

  const fit = useCallback(() => {
    const el = stage.current;
    if (!el) return MIN_ZOOM;
    return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Math.floor(el.clientWidth / (image.width * pixelRatio))));
  }, [image.width, pixelRatio]);

  // The first picture of a page opens at the largest zoom that fits the stage.
  useEffect(() => {
    if (!fitted.current) {
      fitted.current = true;
      setZoom(fit());
      const el = stage.current;
      if (el) el.scrollLeft = (image.width * fit() * pixelRatio - el.clientWidth) / 2;
    }
  }, [fit, image.width, pixelRatio]);

  useEffect(() => {
    const ctx = canvas.current?.getContext("2d");
    if (!ctx) return;
    ctx.imageSmoothingEnabled = false;
    ctx.putImageData(new ImageData(image.data, image.width, image.height), 0, 0);
  }, [image]);

  // The outline of the hovered piece: its edge pixels, found from the hit map.
  useEffect(() => {
    const ctx = outline.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, image.width, image.height);
    if (hover === null) return;
    const tag = hover + 1;
    const lines = ctx.createImageData(image.width, image.height);
    for (let y = 0; y < image.height; y++) {
      for (let x = 0; x < image.width; x++) {
        const at = y * image.width + x;
        if (hit[at] !== tag) continue;
        const edge =
          x === 0 ||
          y === 0 ||
          x === image.width - 1 ||
          y === image.height - 1 ||
          hit[at - 1] !== tag ||
          hit[at + 1] !== tag ||
          hit[at - image.width] !== tag ||
          hit[at + image.width] !== tag;
        if (!edge) continue;
        for (let dy = -2; dy <= 2; dy++)
          for (let dx = -2; dx <= 2; dx++) {
            const px = x + dx,
              py = y + dy;
            if (px < 0 || py < 0 || px >= image.width || py >= image.height) continue;
            lines.data.set(
              Math.abs(dx) === 2 || Math.abs(dy) === 2 ? [27, 29, 43, 255] : [255, 209, 102, 255],
              (py * image.width + px) * 4,
            );
          }
      }
    }
    ctx.putImageData(lines, 0, 0);
  }, [hover, hit, image.width, image.height]);

  const zoomTo = useCallback(
    (next: number) => {
      const el = stage.current;
      const clamped = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next));
      if (el && clamped !== zoom) {
        centre.current = [
          (el.scrollLeft + el.clientWidth / 2) / scale,
          (el.scrollTop + el.clientHeight / 2) / scale,
        ];
      }
      setZoom(clamped);
    },
    [zoom, scale],
  );

  // Keeps the middle of the view on the same pixel of the room when the zoom changes.
  useLayoutEffect(() => {
    const el = stage.current;
    const c = centre.current;
    if (!el || !c) return;
    centre.current = null;
    el.scrollLeft = c[0] * scale - el.clientWidth / 2;
    el.scrollTop = c[1] * scale - el.clientHeight / 2;
  }, [scale]);

  // Ctrl or Cmd + wheel zooms; a plain wheel keeps scrolling the page.
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      zoomTo(zoom + (event.deltaY < 0 ? 1 : -1));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoom, zoomTo]);

  const pick = (clientX: number, clientY: number) => {
    const rect = canvas.current?.getBoundingClientRect();
    if (!rect) return null;
    const x = Math.floor((clientX - rect.left) / scale);
    const y = Math.floor((clientY - rect.top) / scale);
    if (x < 0 || y < 0 || x >= image.width || y >= image.height) return null;
    const tag = hit[y * image.width + x] ?? 0;
    return tag > 0 ? tag - 1 : null;
  };

  return (
    <div className="room-stage">
      <div className="room-toolbar" role="toolbar" aria-label={spaTranslate("Zoom")}>
        <PixelButton
          variant="secondary"
          onClick={() => zoomTo(zoom - 1)}
          disabled={zoom <= MIN_ZOOM}
          aria-label={spaTranslate("Zoom out")}
        >
          −
        </PixelButton>
        <span className="pixel-label room-zoom" aria-live="polite">
          {zoom}×
        </span>
        <PixelButton
          variant="secondary"
          onClick={() => zoomTo(zoom + 1)}
          disabled={zoom >= MAX_ZOOM}
          aria-label={spaTranslate("Zoom in")}
        >
          +
        </PixelButton>
        <PixelButton variant="ghost" onClick={() => zoomTo(fit())}>
          {" "}
          {spaTranslate("Fit")}{" "}
        </PixelButton>
      </div>
      <div
        /* eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- The editing canvas accepts keyboard focus. */
        tabIndex={editor ? 0 : undefined}
        ref={stage}
        className="room-viewport"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          if (editor && !event.altKey) {
            event.currentTarget.focus({ preventScroll: true });
            const rect = canvas.current?.getBoundingClientRect();
            if (rect) editor.point((event.clientX - rect.left) / scale, (event.clientY - rect.top) / scale);
            editor.down(pick(event.clientX, event.clientY));
            event.currentTarget.setPointerCapture(event.pointerId);
            return;
          }
          if (event.pointerType !== "mouse") return;
          const el = stage.current;
          if (!el) return;
          drag.current = { x: event.clientX, y: event.clientY, left: el.scrollLeft, top: el.scrollTop };
          el.setPointerCapture(event.pointerId);
          el.classList.add("is-panning");
        }}
        onPointerMove={(event) => {
          const d = drag.current;
          const el = stage.current;
          if (d && el) {
            el.scrollLeft = d.left - (event.clientX - d.x);
            el.scrollTop = d.top - (event.clientY - d.y);
            return;
          }
          if (editor) {
            const rect = canvas.current?.getBoundingClientRect();
            if (rect) editor.point((event.clientX - rect.left) / scale, (event.clientY - rect.top) / scale);
          }
          onHover(pick(event.clientX, event.clientY));
        }}
        onPointerUp={(event) => {
          if (editor && !drag.current) editor.up();
          drag.current = null;
          stage.current?.classList.remove("is-panning");
          if (event.pointerType !== "mouse") onHover(pick(event.clientX, event.clientY));
        }}
        onPointerCancel={() => {
          drag.current = null;
          editor?.cancel();
        }}
        onPointerLeave={() => onHover(null)}
      >
        <div className="room-sheet" style={{ width: image.width * scale, height: image.height * scale }}>
          <canvas
            ref={canvas}
            className="room-canvas"
            width={image.width}
            height={image.height}
            role="img"
            aria-label={label}
          />
          <canvas
            ref={outline}
            className="room-canvas room-outline"
            width={image.width}
            height={image.height}
            aria-hidden="true"
          />
          {editor?.overlay}
          {botPicture && (
            <img
              className="room-bot"
              src={botPicture}
              alt={spaTranslate("The same room as Discord draws it")}
            />
          )}
        </div>
      </div>
    </div>
  );
}
