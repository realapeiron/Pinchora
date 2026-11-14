import { useEffect, useLayoutEffect, type Dispatch, type RefObject, type SetStateAction } from "react";
import { type CardDesign, type Layer, type History, redo, undo } from "../../../lib/card";
import { type Draft, ZOOMS } from "../pages/welcome/welcomeDesign";

interface InteractionOptions {
  layer: Layer | null;
  dialog: string | null;
  setHist: Dispatch<SetStateAction<History<Draft>>>;
  duplicate: (layer: Layer) => void;
  removeLayer: (id: string) => void;
  setLayer: (layer: Layer, merge?: string) => void;
  setSelected: Dispatch<SetStateAction<string | null>>;
  viewport: RefObject<HTMLDivElement | null>;
  design: CardDesign;
  setFitZoom: Dispatch<SetStateAction<number>>;
  scale: number;
  setZoom: Dispatch<SetStateAction<number | "fit">>;
}

export function useWelcomeInteractions({
  layer,
  dialog,
  setHist,
  duplicate,
  removeLayer,
  setLayer,
  setSelected,
  viewport,
  design,
  setFitZoom,
  scale,
  setZoom,
}: InteractionOptions) {
  // /Keyboard: undo, redo, nudge, delete, duplicate.

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (target.closest("input, textarea, select, [contenteditable]") || dialog) return;
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setHist((h) => (event.shiftKey ? redo(h) : undo(h)));
      } else if (mod && event.key.toLowerCase() === "y") {
        event.preventDefault();
        setHist(redo);
      } else if (layer && mod && event.key.toLowerCase() === "d") {
        event.preventDefault();
        duplicate(layer);
      } else if (layer && (event.key === "Delete" || event.key === "Backspace")) {
        event.preventDefault();
        removeLayer(layer.id);
      } else if (layer && !layer.locked && event.key.startsWith("Arrow")) {
        event.preventDefault();
        const step = event.shiftKey ? 10 : 1;
        const moves: Record<string, [number, number]> = {
          ArrowLeft: [-step, 0],
          ArrowRight: [step, 0],
          ArrowUp: [0, -step],
          ArrowDown: [0, step],
        };
        const [dx, dy] = moves[event.key] ?? [0, 0];
        setLayer({ ...layer, x: layer.x + dx, y: layer.y + dy }, "nudge");
      } else if (event.key === "Escape") {
        setSelected(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // /Zoom: "fit" follows the space the canvas has.

  useLayoutEffect(() => {
    const box = viewport.current;
    if (!box) return;
    const measure = () => setFitZoom(Math.min(1, Math.max(0.1, (box.clientWidth - 8) / design.width)));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    return () => observer.disconnect();
  }, [design.width]);

  const zoomBy = (direction: 1 | -1) => {
    const now = scale;
    const next =
      direction > 0 ? ZOOMS.find((z) => z > now + 0.001) : [...ZOOMS].reverse().find((z) => z < now - 0.001);
    if (next) setZoom(next);
  };

  return { zoomBy };
}
