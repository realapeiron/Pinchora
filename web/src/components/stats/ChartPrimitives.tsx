import { type KeyboardEvent, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { compact } from "../../features/admin/api/stats";

/** The width an element has now, kept up to date as the page resizes. */
export function useWidth<T extends HTMLElement>(fallback: number) {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(fallback);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    setWidth(element.clientWidth || fallback);
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(Math.round(entry.contentRect.width) || fallback);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [fallback]);
  return { ref, width };
}

/** Moves a highlighted index with the arrow keys, Home and End; Escape lets go. */
export function useCursor(count: number) {
  const [active, setActiveIndex] = useState<number | null>(null);
  // Only a move made with the keyboard is read out by a screen reader: a pointer sweeping over the
  // columns would read out every one of them.
  const [spoken, setSpoken] = useState(false);
  const setActive = (index: number | null) => {
    setSpoken(false);
    setActiveIndex(index);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    const keys: Record<string, (i: number) => number> = {
      ArrowRight: (i) => Math.min(count - 1, i + 1),
      ArrowLeft: (i) => Math.max(0, i - 1),
      Home: () => 0,
      End: () => count - 1,
    };
    if (event.key === "Escape") return setActive(null);
    const move = keys[event.key];
    if (!move || count === 0) return;
    event.preventDefault();
    setSpoken(true);
    setActiveIndex((i) => move(i ?? count - 1));
  };
  return { active, setActive, onKeyDown, spoken };
}

/** The floating readout beside the crosshair (left of it near the right edge): the value leads, the
 *  label follows. It sits inside the chart, so the panel around it never cuts it off. */
export function Readout({
  x,
  width,
  spoken,
  children,
}: {
  x: number;
  width: number;
  spoken: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(x + 12);
  useEffect(() => {
    const box = ref.current?.offsetWidth ?? 120;
    setLeft(x + 12 + box <= width ? x + 12 : Math.max(0, x - 12 - box));
  }, [x, width, children]);
  return (
    <div ref={ref} className="chart-readout" style={{ left }} aria-live={spoken ? "polite" : "off"}>
      {children}
    </div>
  );
}

/** A line key in the series' colour, for readouts and legends. */
export function Key({ series, shape = "line" }: { series: 1 | 2 | 3; shape?: "line" | "box" }) {
  return <span className={`chart-key chart-key-${shape} series-${series}`} aria-hidden="true" />;
}

export const PAD = { left: 44, right: 8, top: 26, bottom: 30 };

/** Gridlines at the top, halfway and the base, with what they stand for. */
export function Grid({
  top,
  width,
  height,
  ticks,
}: {
  top: number;
  width: number;
  height: number;
  ticks: boolean;
}) {
  const lines = [
    { y: PAD.top, value: top },
    { y: PAD.top + (height - PAD.top - PAD.bottom) / 2, value: top / 2 },
    { y: height - PAD.bottom, value: 0 },
  ];
  return (
    <g className="chart-grid">
      {lines.map((line, i) => (
        <g key={i}>
          <rect
            x={PAD.left}
            y={line.y - (i === 2 ? 1 : 0.5)}
            width={width - PAD.left - PAD.right}
            height={i === 2 ? 2 : 1}
            className={i === 2 ? "chart-base" : "chart-line"}
          />
          {ticks && (i !== 1 || top % 2 === 0) && (
            <text x={PAD.left - 8} y={line.y + 4} textAnchor="end" className="chart-tick">
              {compact(line.value)}
            </text>
          )}
        </g>
      ))}
    </g>
  );
}

/** A column with its top corners cut by one step: the cards' pixel "rounded" end. */
export function columnPath(x: number, y: number, w: number, h: number) {
  const c = w >= 6 && h >= 4 ? 2 : 0;
  return `M${x + c},${y}H${x + w - c}V${y + c}H${x + w}V${y + h}H${x}V${y + c}H${x + c}Z`;
}
