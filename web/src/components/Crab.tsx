// The Pinchora crab from the cards' footer, drawn pixel by pixel.

const ROWS = ["#.......#", "##.....##", ".#.o.o.#.", ".#######.", "#########", ".#.#.#.#."];

export function Crab({ scale = 3, className }: { scale?: number; className?: string }) {
  const cells: { x: number; y: number; eye: boolean }[] = [];
  ROWS.forEach((row, y) =>
    [...row].forEach((c, x) => {
      if (c !== ".") cells.push({ x, y, eye: c === "o" });
    }),
  );
  return (
    <svg
      className={className}
      width={9 * scale}
      height={6 * scale}
      viewBox="0 0 9 6"
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      {cells.map(({ x, y, eye }) => (
        <rect
          key={`${x}-${y}`}
          x={x}
          y={y}
          width={1}
          height={1}
          className={eye ? "crab-eye" : "crab-shell"}
        />
      ))}
    </svg>
  );
}
