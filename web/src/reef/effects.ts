import type { Point } from "./manifest";

/** A fixed pool: no sprite decoding, timers or per-step particle allocation. */
export class Footsteps {
  readonly particles = Array.from({ length: 12 }, () => ({ x: 0, y: 0, age: 500 }));
  private next = 0;
  private elapsed = 0;
  update(dt: number, walking: boolean, reduced: boolean, position: Point | null): void {
    for (const p of this.particles) p.age = reduced ? 500 : p.age + dt;
    this.elapsed += dt;
    if (!walking || reduced || !position || this.elapsed < 120) return;
    this.elapsed = 0;
    const p = this.particles[this.next++ % this.particles.length]!;
    p.x = 32 * (position[0] - position[1]);
    p.y = 16 * (position[0] + position[1]) + 23;
    p.age = 0;
  }
  draw(ctx: CanvasRenderingContext2D, origin: Point): void {
    ctx.fillStyle = "#fff1e8";
    for (const p of this.particles) {
      if (p.age >= 400) continue;
      ctx.globalAlpha = (1 - p.age / 400) * 0.5;
      const drift = Math.floor(p.age / 100);
      ctx.fillRect(Math.round(p.x) + origin[0] - 7 - drift, Math.round(p.y) + origin[1] - drift, 2, 2);
      ctx.fillRect(Math.round(p.x) + origin[0] + 6 + drift, Math.round(p.y) + origin[1] - drift, 2, 2);
    }
    ctx.globalAlpha = 1;
  }
}
