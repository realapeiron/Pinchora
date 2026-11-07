import { useEffect, useRef, useState } from "react";
import { PixelButton } from "../components/PixelButton";
import type { RoomJson } from "../iso/scene";
import { bootstrap } from "./bootstrap";
import { poseAt } from "./animation";
import { Camera, Gesture } from "./camera";
import { actorItem, Compositor } from "./compositor";
import type { Point } from "./manifest";
import { findPath, type Target, type WalkTarget } from "./navigation";
import { Player } from "./player";
import "./reef.css";

interface Controls {
  walkTo: (id: string) => void;
  move: (p: Point, target?: Target) => void;
  zoom: (delta: number) => void;
  center: () => void;
}
export default function ReefStage({ room, own }: { room: RoomJson; own: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const controls = useRef<Controls | null>(null);
  const [status, setStatus] = useState("Loading your character…");
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [targets, setTargets] = useState<Target[]>([]);
  const [objects, setObjects] = useState<WalkTarget[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [tile, setTile] = useState<Point>([0, 0]);
  const [streaming, setStreaming] = useState(true);
  const [bytes, setBytes] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    let dispose = () => {};
    let data: Awaited<ReturnType<typeof bootstrap>> | null = null;
    if (canvas.current) canvas.current.dataset.ready = "false";
    setReady(false);
    setError(null);
    setStreaming(true);
    setTargets([]);
    setWarnings([]);
    void (async () => {
      const loaded = await bootstrap(room, own, abort.signal);
      data = loaded;
      if (abort.signal.aborted) {
        loaded.dispose();
        return;
      }
      const el = canvas.current!;
      const ctx = el.getContext("2d")!;
      let compositor = new Compositor(await loaded.preparation.prepare(loaded.makeScene()));
      if (abort.signal.aborted) {
        loaded.dispose();
        compositor.dispose();
        return;
      }
      const player = new Player(loaded.nav.grid);
      const camera = new Camera(window.devicePixelRatio || 1);
      const gesture = new Gesture();
      const actor = actorItem();
      const motion = matchMedia("(prefers-reduced-motion: reduce)");
      let raf = 0,
        last = 0,
        pinch = 0,
        announced = "",
        focused = false;
      const frames: number[] = [];
      let ticks = 0;
      const center = () => {
        const p = player.position;
        const [ox, oy] = compositor.room.origin;
        const point: Point = p
          ? [ox + 32 * (p[0] - p[1]), oy + 16 * (p[0] + p[1]) - 30]
          : [compositor.room.background.width / 2, compositor.room.background.height / 2];
        camera.center(el.width / camera.dpr, el.height / camera.dpr, point);
      };
      const resize = () => {
        camera.dpr = window.devicePixelRatio || 1;
        const rect = el.parentElement!.getBoundingClientRect();
        el.width = Math.floor(rect.width * camera.dpr);
        el.height = Math.floor(rect.height * camera.dpr);
        el.style.width = `${el.width / camera.dpr}px`;
        el.style.height = `${el.height / camera.dpr}px`;
        el.style.transform = `translate(${(Math.round(rect.left * camera.dpr) - rect.left * camera.dpr) / camera.dpr}px, ${(Math.round(rect.top * camera.dpr) - rect.top * camera.dpr) / camera.dpr}px)`;
        center();
      };
      const observer = new ResizeObserver(resize);
      observer.observe(el.parentElement!);
      resize();
      const announce = () => {
        if (announced !== player.message) {
          announced = player.message;
          setStatus(announced);
        }
        if (player.tile) el.dataset.tile = player.tile.join(",");
        el.dataset.state = player.state;
      };
      const move = (p: Point, t?: Target) => {
        player.move(p, t);
        announce();
      };
      const zoom = (delta: number) =>
        camera.zoom(camera.scale + delta, [el.width / camera.dpr / 2, el.height / camera.dpr / 2]);
      const walkTo = (id: string) => {
        const object = loaded.nav.objects.find((o) => o.id === id);
        if (!object || !player.tile) return;
        const routes = object.approaches
          .map((p) => findPath(player.grid, player.tile!, p))
          .filter((p) => p !== null)
          .sort((a, b) => a.length - b.length);
        const goal = routes[0]?.at(-1);
        if (goal) move(goal);
        else {
          player.message = "That destination is unreachable.";
          announce();
        }
      };
      controls.current = { move, zoom, center, walkTo };
      const local = (e: PointerEvent): Point => {
        const r = el.getBoundingClientRect();
        return [e.clientX - r.left, e.clientY - r.top];
      };
      const down = (e: PointerEvent) => {
        if (e.button !== 0) return;
        el.focus({ preventScroll: true });
        gesture.down(e.pointerId, local(e));
        el.setPointerCapture(e.pointerId);
      };
      const drag = (e: PointerEvent) => {
        const result = gesture.move(e.pointerId, local(e));
        if (!result) return;
        camera.pan(...result.pan);
        pinch += result.pinch;
        if (Math.abs(pinch) >= 40) {
          camera.zoom(camera.scale + Math.sign(pinch), local(e));
          pinch = 0;
        }
      };
      const up = (e: PointerEvent) => {
        if (!gesture.up(e.pointerId)) return;
        const p = local(e);
        const [x, y] = camera.scene(...p);
        const index = compositor.pick(x, y);
        const item = index === null ? null : compositor.room.scene.items[index];
        const target = loaded.nav.targets.find((t) => t.id === item?.info.placement);
        if (target) move(target.anchor, target);
        else if (item?.info.placement) walkTo(item.info.placement);
        else {
          const tile = camera.tile(...p, compositor.room.origin, room.width, room.depth);
          if (tile) move(tile);
        }
      };
      const cancel = () => {
        gesture.cancel();
        pinch = 0;
      };
      const wheel = (e: WheelEvent) => {
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          const r = el.getBoundingClientRect();
          camera.zoom(camera.scale + (e.deltaY < 0 ? 1 : -1), [e.clientX - r.left, e.clientY - r.top]);
        }
      };
      const keydown = (e: KeyboardEvent) => {
        const steps: Record<string, Point> = {
          ArrowRight: [1, 0],
          ArrowDown: [0, 1],
          ArrowLeft: [-1, 0],
          ArrowUp: [0, -1],
        };
        const step = steps[e.key];
        if (step && player.tile) {
          e.preventDefault();
          move([player.tile[0] + step[0], player.tile[1] + step[1]]);
        }
        if (e.key === "Home") {
          e.preventDefault();
          center();
        }
      };
      const focus = () => {
        focused = true;
      };
      const blur = () => {
        focused = false;
      };
      el.addEventListener("pointerdown", down);
      el.addEventListener("pointermove", drag);
      el.addEventListener("pointerup", up);
      el.addEventListener("pointercancel", cancel);
      el.addEventListener("lostpointercapture", cancel);
      el.addEventListener("wheel", wheel, { passive: false });
      el.addEventListener("keydown", keydown);
      el.addEventListener("focus", focus);
      el.addEventListener("blur", blur);
      const frame = (now: number) => {
        const started = performance.now();
        const dt = last ? Math.min(100, now - last) : 0;
        last = now;
        player.update(dt, motion.matches);
        announce();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.fillStyle = "#121420";
        ctx.fillRect(0, 0, el.width, el.height);
        ctx.setTransform(camera.scale, 0, 0, camera.scale, camera.x, camera.y);
        const pos = player.state === "sit" && player.target ? player.target.anchor : player.position;
        if (pos) {
          actor.x = pos[0];
          actor.y = pos[1];
          const f = loaded.animation.get(
            player.direction,
            poseAt(player.state, player.elapsed, motion.matches),
          );
          const lift =
            player.state === "sit" && player.target
              ? 122 - player.target.height - (f.meta.seat_contact?.[1] ?? 101)
              : 8;
          compositor.draw(ctx, [{ item: actor, image: f.image, offset: [-f.padding, lift - f.padding] }]);
          if (focused) {
            ctx.strokeStyle = "#ffd166";
            ctx.lineWidth = 1;
            const [ox, oy] = compositor.room.origin;
            ctx.strokeRect(
              Math.round(32 * (pos[0] - pos[1])) + ox - 17,
              Math.round(16 * (pos[0] + pos[1])) + oy + 14,
              34,
              14,
            );
          }
        } else compositor.draw(ctx);
        frames.push(performance.now() - started);
        if (frames.length > 600) frames.shift();
        if (++ticks % 60 === 0) {
          const sorted = [...frames].sort((a, b) => a - b);
          el.dataset.frameP95 = String(sorted[Math.floor(sorted.length * 0.95)] ?? 0);
          el.dataset.cacheBytes = String(loaded.loader.cache.bytes);
        }
        raf = requestAnimationFrame(frame);
      };
      raf = requestAnimationFrame(frame);
      setReady(Boolean(player.tile));
      setTargets(loaded.nav.targets);
      setObjects(loaded.nav.objects);
      setWarnings(loaded.warnings);
      announce();
      loaded.ledger.markPlayable();
      setBytes(loaded.ledger.firstInteraction!);
      el.dataset.ready = "true";
      dispose = () => {
        cancelAnimationFrame(raf);
        observer.disconnect();
        controls.current = null;
        el.removeEventListener("pointerdown", down);
        el.removeEventListener("pointermove", drag);
        el.removeEventListener("pointerup", up);
        el.removeEventListener("pointercancel", cancel);
        el.removeEventListener("lostpointercapture", cancel);
        el.removeEventListener("wheel", wheel);
        el.removeEventListener("keydown", keydown);
        el.removeEventListener("focus", focus);
        el.removeEventListener("blur", blur);
        compositor.dispose();
        loaded.dispose();
      };
      try {
        await loaded.stream(async () => {
          const next = new Compositor(await loaded.preparation.prepare(loaded.makeScene()));
          // Bounds can grow when a tall sprite streams in; keep world coordinates fixed on screen.
          camera.x += (compositor.room.origin[0] - next.room.origin[0]) * camera.scale;
          camera.y += (compositor.room.origin[1] - next.room.origin[1]) * camera.scale;
          compositor.dispose();
          compositor = next;
        });
        if (!abort.signal.aborted) setStreaming(false);
      } catch (err) {
        if (!abort.signal.aborted) {
          setStreaming(false);
          setWarnings((w) => [...w, `Some room art could not load: ${String(err)}`]);
        }
      }
    })().catch((err) => {
      if (!abort.signal.aborted) {
        data?.dispose();
        setError(err instanceof Error ? err.message : String(err));
      }
    });
    return () => {
      abort.abort();
      dispose();
    };
  }, [room, own]);
  return (
    <section className="reef" aria-label="Play in this room">
      <div className="room-toolbar" role="toolbar" aria-label="Play camera">
        <PixelButton
          variant="secondary"
          disabled={!ready}
          onClick={() => controls.current?.zoom(-1)}
          aria-label="Zoom out"
        >
          −
        </PixelButton>
        <PixelButton
          variant="secondary"
          disabled={!ready}
          onClick={() => controls.current?.zoom(1)}
          aria-label="Zoom in"
        >
          +
        </PixelButton>
        <PixelButton variant="secondary" disabled={!ready} onClick={() => controls.current?.center()}>
          Centre on me
        </PixelButton>
        <span className="muted">Play · local preview</span>
      </div>
      {error && <p role="alert">{error} Switch to View to see the room.</p>}
      <div className="reef-surface">
        <canvas
          ref={canvas}
          className="reef-canvas"
          tabIndex={0}
          role="application"
          aria-label="Playable room. Arrow keys move one tile. Home centres the camera."
          aria-describedby="reef-help"
        />
      </div>
      <p id="reef-help" className="muted">
        Tap or click to walk or interact. Drag to pan; pinch or use + / − to zoom. Arrow keys move one tile.
      </p>
      <p role="status" aria-live="polite">
        {status}
      </p>
      {ready && (
        <p className="muted">
          {streaming ? "Decorative art is loading; movement is ready." : "Room art loaded."} Art and data to
          play: {Math.ceil(bytes / 1000)} KB.
        </p>
      )}
      {warnings.map((w) => (
        <p key={w} role="status">
          {w}
        </p>
      ))}
      <form
        className="reef-walk"
        onSubmit={(e) => {
          e.preventDefault();
          controls.current?.move(tile);
        }}
      >
        <label>
          Tile X{" "}
          <input
            type="number"
            min={0}
            max={room.width - 1}
            value={tile[0]}
            onChange={(e) => setTile([Number(e.target.value), tile[1]])}
          />
        </label>
        <label>
          Tile Y{" "}
          <input
            type="number"
            min={0}
            max={room.depth - 1}
            value={tile[1]}
            onChange={(e) => setTile([tile[0], Number(e.target.value)])}
          />
        </label>
        <PixelButton type="submit" disabled={!ready}>
          Walk to tile
        </PixelButton>
      </form>
      <details>
        <summary>Furniture and actions</summary>
        <ul className="reef-objects">
          {room.placements.map((p) => {
            const target = targets.find((t) => t.id === p.id);
            return (
              <li key={p.id}>
                <span>
                  {objects.find((o) => o.id === p.id)?.label ?? p.item_id} ({p.x}, {p.y})
                </span>
                <PixelButton
                  variant="secondary"
                  disabled={!ready}
                  onClick={() =>
                    target ? controls.current?.move(target.anchor, target) : controls.current?.walkTo(p.id)
                  }
                >
                  {target?.kind === "seat" ? "Sit" : target?.kind === "usable" ? "Use" : "Walk to"}
                </PixelButton>
              </li>
            );
          })}
        </ul>
      </details>
    </section>
  );
}
