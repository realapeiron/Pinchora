import type { Footsteps } from "./effects";
import type { Point } from "./manifest";
export function drawGround(
  ctx: CanvasRenderingContext2D,
  origin: Point,
  destination: Point | null,
  markerAge: number,
  reducedMotion: boolean,
  dust: Footsteps,
) {
  const [ox, oy] = origin;
  if (destination && markerAge < 2400) {
    const x = ox + 32 * (destination[0] - destination[1]);
    const y = oy + 16 * (destination[0] + destination[1]);
    ctx.globalAlpha = reducedMotion ? 0.55 : 0.45 + Math.sin(markerAge / 160) * 0.15;
    ctx.fillStyle = "#ffd166";
    ctx.strokeStyle = "#fff1e8";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(x, y + 2);
    ctx.lineTo(x + 28, y + 16);
    ctx.lineTo(x, y + 30);
    ctx.lineTo(x - 28, y + 16);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.globalAlpha = 1;
  }
  dust.draw(ctx, origin);
}
