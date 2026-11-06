/** The approved mascot, shared by the landing, dashboard and brand kit. */
export function Crab({ scale = 3, className }: { scale?: number; className?: string }) {
  return (
    <img
      src="/mascot.png"
      className={className}
      width={9 * scale}
      height={6 * scale}
      style={{ imageRendering: "pixelated", objectFit: "contain" }}
      alt=""
      aria-hidden="true"
    />
  );
}
