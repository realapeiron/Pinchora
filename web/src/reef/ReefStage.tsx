import { SEAT_FOREGROUND_ROWS } from "./seats";
import type { EditBridge } from "./DecoratePanel";
import { PageHead } from "../pages/PageHead";
import type { HouseTarget } from "../lib/house";
import { Art } from "../iso/art";
import type { SceneItem } from "../iso/scene";
import { footprint, surfaceLift } from "../iso/rules";
import { anchor } from "../iso/render";
import { levelTop, WALL_LEVELS } from "../iso/projection";
import { flipHorizontal } from "../iso/image";
import { bitmap, isStanding } from "./compositor";
import { translate as spaTranslate } from "../i18n";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { PixelButton } from "../components/PixelButton";
import type { RoomJson } from "../iso/scene";
import { bootstrap } from "./bootstrap";
import { ChatPanel } from "./ChatPanel";
import { ReefNet, type NetworkState } from "./net";
import { poseAt, type Animation } from "./animation";
import { Camera, Gesture } from "./camera";
import { actorItem, Compositor, type ActorDraw } from "./compositor";
import type { Point } from "./manifest";
import { findPath, type Target, type WalkTarget } from "./navigation";
import { Player } from "./player";
import { Footsteps } from "./effects";
import { t, playerStatus } from "./i18n";
import "./reef.css";

const DecoratePanel = lazy(() => import("./DecoratePanel").then((m) => ({ default: m.DecoratePanel })));
const SpacesPanel = lazy(() => import("./SpacesPanel").then((m) => ({ default: m.SpacesPanel })));

interface Controls {
  walkTo: (id: string) => void;
  move: (p: Point, target?: Target) => void;
  zoom: (delta: number) => void;
  center: () => void;
}
export default function ReefStage({
  room,
  own,
  roomKey,
  onLayout,
  target,
  editable,
  userId,
  roomName,
  onExit,
  rooms,
  onRoom,
}: {
  target: HouseTarget;
  editable: boolean;
  userId: string;
  room: RoomJson;
  own: boolean;
  roomKey: string;
  onLayout: () => void;
  roomName: string;
  playerName: string;
  onExit: () => void;
  rooms: { idx: number; name: string }[];
  onRoom: (idx: number) => void;
}) {
  const [decorating, setDecorating] = useState(false);
  const [social, setSocial] = useState(false);
  const editBridge = useRef<EditBridge | null>(null);
  const refreshLayout = useRef(onLayout);
  refreshLayout.current = onLayout;
  const canvas = useRef<HTMLCanvasElement>(null);
  const controls = useRef<Controls | null>(null);
  const nameTag = useRef<HTMLDivElement>(null);
  const bubble = useRef<HTMLDivElement>(null);
  const drawerToggle = useRef<HTMLButtonElement>(null);
  const drawerClose = useRef<HTMLButtonElement>(null);
  const [drawer, setDrawer] = useState(() => !matchMedia("(pointer: coarse), (max-width: 767px)").matches);
  const [selected, setSelected] = useState<Target | null>(null);
  const closeDrawer = () => {
    setDrawer(false);
    drawerToggle.current?.focus({ preventScroll: true });
  };
  const openDrawer = () => {
    setDrawer(true);
    requestAnimationFrame(() => drawerClose.current?.focus({ preventScroll: true }));
  };
  const [status, setStatus] = useState(spaTranslate("Loading your character…"));
  const [network, setNetwork] = useState<NetworkState>("preview");
  const controller = useRef<ReefNet | null>(null);
  const [chatNet, setChatNet] = useState<ReefNet | null>(null);
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
    setNetwork("preview");
    setReady(false);
    setError(null);
    setStreaming(true);
    setTargets([]);
    setSelected(null);
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
      let compositor = new Compositor(await loaded.preparation.prepare(loaded.makeScene(), true));
      if (abort.signal.aborted) {
        loaded.dispose();
        compositor.dispose();
        return;
      }
      const player = new Player(loaded.nav.grid);
      const remoteArt = new Map<string, Animation>();
      const pendingArt = new Set<string>();
      const tags = new Map<string, HTMLDivElement>();
      const chatBubbles = new Map<string, HTMLButtonElement>();
      let wasOnline = false;
      const net = new ReefNet(
        roomKey,
        (message) => {
          el.dataset.network = net.network;
          setNetwork(net.network);
          player.message = message;
          if (!net.ready && wasOnline && player.tile)
            player.synchronize(player.tile, [...player.tile], player.direction, "idle", null);
          wasOnline = net.ready;
          setStatus(message);
        },
        () => refreshLayout.current(),
        room,
      );
      controller.current = net;
      setChatNet(net);
      void net.connect();
      const camera = new Camera(window.devicePixelRatio || 1);
      const gesture = new Gesture();
      const actor = actorItem();
      const seatMask = (id: string | undefined) => {
        const piece = room.placements.find((p) => p.id === id);
        const base = piece ? loaded.content.item(piece.item_id)?.base.id : undefined;
        return id && base && SEAT_FOREGROUND_ROWS[base]
          ? { id, rows: SEAT_FOREGROUND_ROWS[base]! }
          : undefined;
      };
      const motion = matchMedia("(prefers-reduced-motion: reduce)");
      let raf = 0,
        last = 0,
        pinch = 0,
        announced = "",
        focused = false;
      const frames: number[] = [];
      let ticks = 0;
      const dust = new Footsteps();
      let following = true;
      let keepRoomVisible = true;
      let centering = true;
      let interacted = false;
      let openingTime = 0;
      let destination: Point | null = null;
      let markerAge = 0;
      let contextTarget: Target | null = null;
      const size = (): Point => [el.width / camera.dpr, el.height / camera.dpr];
      const roomSize = (): Point => [compositor.background.width, compositor.background.height];
      const playerPoint = (): Point => {
        const p = player.state === "sit" && player.target ? player.target.anchor : player.position;
        const [ox, oy] = compositor.room.origin;
        return p
          ? [ox + 32 * (p[0] - p[1]), oy + 16 * (p[0] + p[1]) - 30]
          : [roomSize()[0] / 2, roomSize()[1] / 2];
      };
      const fit = () => {
        camera.fit(...size(), roomSize());
        // A low-DPR viewport can be smaller than the native room even at ×1.
        // In that case show the player immediately rather than opening on an empty corner.
        if (
          roomSize()[0] * camera.scale > el.width - 48 * camera.dpr ||
          roomSize()[1] * camera.scale > el.height - 48 * camera.dpr
        )
          camera.follow(...size(), playerPoint(), roomSize(), 0, true, 0);
      };
      const center = () => {
        keepRoomVisible = false;
        following = true;
        centering = true;
        openingTime = 1000;
      };
      const resize = () => {
        camera.dpr = window.devicePixelRatio || 1;
        const rect = el.parentElement!.getBoundingClientRect();
        el.width = Math.floor(rect.width * camera.dpr);
        el.height = Math.floor(rect.height * camera.dpr);
        el.style.width = `${el.width / camera.dpr}px`;
        el.style.height = `${el.height / camera.dpr}px`;
        el.style.transform = `translate(${(Math.round(rect.left * camera.dpr) - rect.left * camera.dpr) / camera.dpr}px, ${(Math.round(rect.top * camera.dpr) - rect.top * camera.dpr) / camera.dpr}px)`;
        fit();
        keepRoomVisible = true;
        following = true;
        centering = true;
      };
      const observer = new ResizeObserver(resize);
      observer.observe(el.parentElement!);
      resize();
      const announce = () => {
        if (!net.ready && announced !== player.message) {
          announced = player.message;
          setStatus(playerStatus(announced));
        }
        if (player.tile) el.dataset.tile = player.tile.join(",");
        el.dataset.state = player.state;
      };
      const move = (p: Point, t?: Target) => {
        interacted = true;
        if (net.ready) {
          const start = net.players.get(net.userId)?.tile;
          const route =
            t && start
              ? t.approaches
                  .map((tile) => findPath(player.grid, start, tile))
                  .filter((path) => path !== null)
                  .sort((a, b) => a.length - b.length)[0]
              : null;
          const goal = t ? route?.at(-1) : p;
          if (goal) net.move(goal, t ? { id: t.id, tile: goal } : undefined);
        }
        if (player.move(p, net.ready ? undefined : t)) {
          destination = p;
          markerAge = 0;
          following = true;
          keepRoomVisible = false;
          centering = false;
          openingTime = 1801;
        }
        contextTarget = null;
        setSelected(null);
        announce();
      };
      const zoom = (delta: number) => {
        interacted = true;
        centering = false;
        camera.zoom(camera.scale + delta, [size()[0] / 2, size()[1] / 2]);
      };
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
      const placementTile = (point: Point): Point | null => {
        const ghost = editBridge.current?.ghost;
        if (ghost && loaded.content.item(ghost.item_id)?.base.layer === "wall") {
          const [sx, sy] = camera.scene(...point);
          const x = sx - compositor.room.origin[0],
            y = sy - compositor.room.origin[1];
          if (x < 0 !== ghost.flipped) return null;
          const tile = Math.floor(Math.abs(x) / 32),
            level = Math.floor((y - levelTop(x)) / 32);
          return tile < (ghost.flipped ? room.depth : room.width) && level >= 0 && level < WALL_LEVELS
            ? [tile, level]
            : null;
        }
        return camera.tile(...point, compositor.room.origin, room.width, room.depth);
      };
      const down = (e: PointerEvent) => {
        if (e.button !== 0) return;
        el.focus({ preventScroll: true });
        gesture.down(e.pointerId, local(e));
        el.setPointerCapture(e.pointerId);
      };
      const drag = (e: PointerEvent) => {
        if (editBridge.current && e.pointerType !== "touch" && !e.buttons)
          editBridge.current.pick(placementTile(local(e)), null, true);
        const result = gesture.move(e.pointerId, local(e));
        if (!result) return;
        if (result.pan[0] || result.pan[1] || result.pinch) {
          interacted = true;
          following = false;
          centering = false;
        }
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
        if (editBridge.current) {
          editBridge.current.pick(placementTile(p), item?.info.placement ?? null, false);
          return;
        }
        const target = loaded.nav.targets.find((t) => t.id === item?.info.placement);
        if (target) {
          contextTarget = target;
          setSelected(target);
        } else if (item?.info.placement) walkTo(item.info.placement);
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
          interacted = true;
          following = false;
          centering = false;
          const r = el.getBoundingClientRect();
          camera.zoom(camera.scale + (e.deltaY < 0 ? 1 : -1), [e.clientX - r.left, e.clientY - r.top]);
        }
      };
      const keydown = (e: KeyboardEvent) => {
        if (editBridge.current?.key(e.key)) {
          e.preventDefault();
          return;
        }
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
        if (e.key === "Escape") {
          contextTarget = null;
          setSelected(null);
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
      const ground = () => {
        const [ox, oy] = compositor.room.origin;
        if (destination && markerAge < 2400) {
          const x = ox + 32 * (destination[0] - destination[1]);
          const y = oy + 16 * (destination[0] + destination[1]);
          ctx.globalAlpha = motion.matches ? 0.55 : 0.45 + Math.sin(markerAge / 160) * 0.15;
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
        dust.draw(ctx, compositor.room.origin);
      };
      const placeOverlay = (node: HTMLElement | null, point: Point, lift: number) => {
        if (!node) return;
        const [ox, oy] = compositor.room.origin;
        const x = (camera.x + (ox + 32 * (point[0] - point[1])) * camera.scale) / camera.dpr;
        const y = (camera.y + (oy + 16 * (point[0] + point[1]) - lift) * camera.scale) / camera.dpr;
        node.style.left = `${x}px`;
        node.style.top = `${y}px`;
      };
      let ghostId = "";
      let ghostArt: Art | null = null;
      let ghostBitmap: HTMLCanvasElement | null = null;
      let ghostOrientation = "";
      let ghostPosition = "";
      let ghostItem: SceneItem | null = null;
      const frame = (now: number) => {
        const started = performance.now();
        const dt = last ? Math.min(100, now - last) : 0;
        last = now;
        if (!net.ready) player.update(dt, motion.matches);
        else {
          const me = net.visual(net.userId, now);
          if (me) {
            player.synchronize(
              me.tile,
              motion.matches && !me.seat ? me.tile : me.position,
              me.direction,
              me.state,
              me.seat,
            );
            player.elapsed += dt;
          }
        }
        el.dataset.players = String(net.players.size);
        el.dataset.placements = String(room.placements.length);
        for (const [id, tag] of tags)
          if (!net.players.has(id) || !net.ready) {
            tag.remove();
            tags.delete(id);
          }
        const activeCharacters = new Set([...net.players.values()].map((p) => p.character));
        for (const code of remoteArt.keys())
          if (!activeCharacters.has(code)) {
            loaded.releaseAnimation(code);
            remoteArt.delete(code);
            pendingArt.delete(code);
          }
        const remoteDraws: ActorDraw[] = [];
        if (net.ready)
          for (const id of [...net.players.keys()].sort()) {
            if (id === net.userId) continue;
            const sample = net.visual(id, now)!;
            const remote = motion.matches && !sample.seat ? { ...sample, position: sample.tile } : sample;
            if (!remoteArt.has(remote.character) && !pendingArt.has(remote.character)) {
              pendingArt.add(remote.character);
              void loaded
                .loadAnimation(remote.character)
                .then((animation) => {
                  if (!abort.signal.aborted) remoteArt.set(remote.character, animation);
                })
                .catch(() => {
                  if (!abort.signal.aborted)
                    setWarnings((w) => [...w, spaTranslate("A player's saved appearance could not load.")]);
                });
            }
            const animation = remoteArt.get(remote.character);
            if (!animation) continue;
            const f = animation.get(remote.direction, poseAt(remote.state, now, motion.matches));
            const lift = remote.seat ? 122 - remote.seat.height - (f.meta.seat_contact?.[1] ?? 101) : 8;
            const item = actorItem();
            item.info.label = id;
            item.x = remote.position[0];
            item.y = remote.position[1];
            remoteDraws.push({
              item,
              image: f.image,
              offset: [-f.padding, lift - f.padding],
              seat: seatMask(remote.seat?.id),
            });
            let tag = tags.get(id);
            if (!tag) {
              tag = document.createElement("div");
              tag.className = "reef-name-tag";
              tag.dataset.playerId = id;
              tag.textContent = remote.name;
              el.parentElement!.append(tag);
              tags.set(id, tag);
            }
            placeOverlay(tag, remote.position, 105 - lift);
          }
        announce();
        openingTime += dt;
        markerAge += dt;
        dust.update(dt, player.state === "walk", motion.matches, player.position);
        if (following) {
          if (openingTime > 700 || motion.matches) {
            camera.follow(
              ...size(),
              playerPoint(),
              roomSize(),
              dt,
              motion.matches,
              centering ? 0 : 24,
              keepRoomVisible,
            );
            if (openingTime > 1800) centering = false;
          }
        }
        el.dataset.origin = compositor.room.origin.join(",");
        el.dataset.roomSize = roomSize().join(",");
        el.dataset.scale = String(camera.scale);
        el.dataset.camera = `${camera.x},${camera.y}`;
        el.dataset.cameraSettled = String(openingTime > 1800);
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.clearRect(0, 0, el.width, el.height);
        ctx.setTransform(camera.scale, 0, 0, camera.scale, camera.x, camera.y);
        const pos = player.state === "sit" && player.target ? player.target.anchor : player.position;
        if (pos) {
          actor.info.label = net.userId;
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
          const bob =
            !motion.matches && player.state === "idle" ? Math.round(Math.sin(player.elapsed / 450)) : 0;
          compositor.draw(
            ctx,
            (
              [
                {
                  item: actor,
                  image: f.image,
                  offset: [-f.padding, lift - f.padding + bob],
                  seat: seatMask(player.target?.id),
                },
                ...remoteDraws,
              ] as ActorDraw[]
            ).sort((a, b) => a.item.info.label.localeCompare(b.item.info.label)),
            ground,
          );
          placeOverlay(nameTag.current, pos, 105 - lift - bob);
          if (contextTarget) placeOverlay(bubble.current, contextTarget.anchor, contextTarget.height + 36);
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
        } else compositor.draw(ctx, [], ground);
        const ghost = editBridge.current?.ghost;
        if (ghost) {
          if (ghost.item_id !== ghostId) {
            ghostId = ghost.item_id;
            ghostArt = null;
            if (ghostBitmap) ghostBitmap.width = ghostBitmap.height = 0;
            ghostBitmap = null;
            ghostItem = null;
            ghostPosition = "";
            ghostOrientation = "";
            const requested = ghostId;
            const base = loaded.content.item(requested)?.base.id;
            if (base)
              void loaded.loader
                .sprite(`f_${base}`)
                .then((img) => {
                  if (img && !abort.signal.aborted && ghostId === requested)
                    ghostArt = new Art({ get: () => img, has: () => true }, loaded.content);
                })
                .catch(() => {
                  if (!abort.signal.aborted && ghostId === requested)
                    setWarnings((current) => [
                      ...new Set([...current, spaTranslate("Some room art could not load.")]),
                    ]);
                });
          }
          const positionKey = `${ghostId}:${ghost.x}:${ghost.y}:${ghost.flipped}`;
          if (ghostArt && positionKey !== ghostPosition) {
            const sprite = ghostArt.item(ghostId);
            const fp = footprint(loaded.content, ghostId, ghost.x, ghost.y, ghost.flipped);
            ghostItem =
              sprite && fp
                ? {
                    sprite,
                    ...fp.rect,
                    layer: fp.layer,
                    flipped: ghost.flipped,
                    selected: false,
                    lift:
                      fp.layer === "surface"
                        ? surfaceLift(loaded.content, room.placements, fp.rect.x, fp.rect.y)
                        : 0,
                    info: { kind: "furniture", label: "", detail: null, placement: null },
                  }
                : null;
            ghostPosition = positionKey;
          }
          const item = ghostItem;
          if (item) {
            const key = `${ghostId}:${ghost.flipped}`;
            if (key !== ghostOrientation) {
              if (ghostBitmap) ghostBitmap.width = ghostBitmap.height = 0;
              ghostBitmap = bitmap(
                item.flipped && isStanding(item) ? flipHorizontal(item.sprite) : item.sprite,
              );
              ghostOrientation = key;
            }
            const pos = anchor(item, item.sprite);
            ctx.globalAlpha = 0.6;
            const [ox, oy] = compositor.room.origin;
            if (item.layer === "wall") {
              const sign = ghost.flipped ? -1 : 1;
              ctx.save();
              ctx.transform(
                sign,
                0.5,
                0,
                1,
                ox + sign * item.x * 32,
                oy + levelTop(sign * item.x * 32) + item.y * 32,
              );
              ctx.drawImage(ghostBitmap!, 0, 0, item.w * 32, item.d * 32);
              ctx.restore();
            } else if (item.layer === "rug") {
              const sw = item.sprite.width,
                sh = item.sprite.height;
              ctx.save();
              if (ghost.flipped)
                ctx.transform(
                  (-32 * item.d) / sw,
                  (16 * item.d) / sw,
                  (32 * item.w) / sh,
                  (16 * item.w) / sh,
                  ox + 32 * (item.x - item.y),
                  oy + 16 * (item.x + item.y),
                );
              else
                ctx.transform(
                  (32 * item.w) / sw,
                  (16 * item.w) / sw,
                  (-32 * item.d) / sh,
                  (16 * item.d) / sh,
                  ox + 32 * (item.x - item.y),
                  oy + 16 * (item.x + item.y),
                );
              ctx.drawImage(ghostBitmap!, 0, 0);
              ctx.restore();
            } else ctx.drawImage(ghostBitmap!, Math.round(pos[0]) + ox, Math.round(pos[1]) + oy);
            ctx.globalAlpha = 1;
          }
          const [ox, oy] = compositor.room.origin;
          const x = ghost.x,
            y = ghost.y,
            w = item?.w ?? 1,
            d = item?.d ?? 1;
          const points =
            item?.layer === "wall"
              ? (() => {
                  const sign = ghost.flipped ? -1 : 1,
                    a = sign * 32 * x,
                    b = sign * 32 * (x + w);
                  return [
                    [a, levelTop(a) + y * 32],
                    [b, levelTop(b) + y * 32],
                    [b, levelTop(b) + (y + d) * 32],
                    [a, levelTop(a) + (y + d) * 32],
                  ];
                })()
              : [
                  [32 * (x - y), 16 * (x + y)],
                  [32 * (x + w - y), 16 * (x + w + y)],
                  [32 * (x + w - y - d), 16 * (x + w + y + d)],
                  [32 * (x - y - d), 16 * (x + y + d)],
                ];
          ctx.strokeStyle = ghost.valid ? "#80dbad" : "#ff8e8e";
          ctx.lineWidth = 2;
          ctx.setLineDash(ghost.valid ? [] : [3, 3]);
          ctx.beginPath();
          points.forEach(([px, py], i) =>
            i ? ctx.lineTo(ox + px!, oy + py!) : ctx.moveTo(ox + px!, oy + py!),
          );
          ctx.closePath();
          ctx.stroke();
          ctx.setLineDash([]);
        }
        const latest = new Map(
          net.chat
            .snapshot()
            .filter(
              (line) => line.delivery === "delivered" && Date.now() - Date.parse(line.created_at) < 8000,
            )
            .map((line) => [line.sender_id, line]),
        );
        for (const [id, node] of chatBubbles) {
          if (!latest.has(id) || !net.players.has(id)) {
            node.remove();
            chatBubbles.delete(id);
          }
        }
        for (const [id, line] of latest) {
          const person = net.visual(id, now);
          if (!person) continue;
          let node = chatBubbles.get(id);
          if (!node) {
            node = document.createElement("button");
            node.type = "button";
            node.className = "reef-chat-bubble";
            node.onclick = () =>
              window.dispatchEvent(new CustomEvent("reef-report", { detail: node!.dataset.messageId }));
            el.parentElement!.append(node);
            chatBubbles.set(id, node);
          }
          node.textContent = line.text;
          node.dataset.messageId = line.message_id;
          node.setAttribute("aria-label", `${t("Report message")}: ${line.name}: ${line.text}`);
          placeOverlay(node, person.position, 145);
        }
        if (nameTag.current && net.ready)
          nameTag.current.textContent = net.players.get(net.userId)?.name ?? "🐚";
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
      setWarnings(
        loaded.warnings.length ? [spaTranslate("Some accessories could not be displayed correctly.")] : [],
      );
      announce();
      loaded.ledger.markPlayable();
      setBytes(loaded.ledger.firstInteraction!);
      el.dataset.ready = "true";
      el.dataset.artReady = "false";
      dispose = () => {
        net.dispose();
        if (controller.current === net) controller.current = null;
        for (const tag of tags.values()) tag.remove();
        for (const node of chatBubbles.values()) node.remove();
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
        if (ghostBitmap) ghostBitmap.width = ghostBitmap.height = 0;
        compositor.dispose();
        loaded.dispose();
      };
      try {
        await loaded.stream(async () => {
          const next = new Compositor(await loaded.preparation.prepare(loaded.makeScene(), true));
          if (abort.signal.aborted) {
            next.dispose();
            return;
          }
          // Bounds can grow when a tall sprite streams in; keep world coordinates fixed on screen.
          camera.x += (compositor.room.origin[0] - next.room.origin[0]) * camera.scale;
          camera.y += (compositor.room.origin[1] - next.room.origin[1]) * camera.scale;
          compositor.dispose();
          compositor = next;
          if (!interacted) fit();
        });
        if (!abort.signal.aborted) {
          setStreaming(false);
          el.dataset.artReady = "true";
        }
      } catch {
        if (!abort.signal.aborted) {
          setStreaming(false);
          setWarnings((w) => [...w, spaTranslate("Some room art could not load.")]);
        }
      }
    })().catch((err: unknown) => {
      if (!abort.signal.aborted) {
        data?.dispose();
        setError(
          err instanceof Error && err.message === "character_required"
            ? spaTranslate("Create your character in Discord before playing.")
            : spaTranslate("This room could not load. Please try again."),
        );
      }
    });
    return () => {
      abort.abort();
      dispose();
    };
  }, [room, own, roomKey]);
  return (
    <section className="reef" aria-label={t("Play in this room")} data-controls-open={drawer}>
      <div className="reef-surface">
        <canvas
          ref={canvas}
          className="reef-canvas"
          data-network={network}
          tabIndex={0}
          role="application"
          aria-label={t("Playable room. Arrow keys move one tile. Home centres the camera.")}
          aria-describedby="reef-help"
        />
        <div className="reef-vignette" aria-hidden="true" />
        <div className="reef-room-hud">
          <div className="reef-room-name">
            <PageHead title={roomName} lead={null} />
          </div>
          {editable && (
            <button
              type="button"
              onClick={() => {
                setDecorating(!decorating);
                setSocial(false);
                setDrawer(false);
              }}
            >
              {t("Decorate")}
            </button>
          )}
          <button
            type="button"
            onClick={() => {
              setSocial(!social);
              setDecorating(false);
              setDrawer(false);
            }}
          >
            {t("Social spaces")}
          </button>
          <button type="button" className="reef-text-button" onClick={onExit}>
            {t("View")}
          </button>
        </div>
        <div className="reef-name-tag" ref={nameTag} hidden={!ready}>
          {t("You")}
        </div>
        <div className="reef-context" ref={bubble} hidden={!selected}>
          <button
            type="button"
            onClick={() => {
              if (selected) controls.current?.move(selected.anchor, selected);
              canvas.current?.focus({ preventScroll: true });
            }}
          >
            {selected?.kind === "seat" ? t("Sit") : t("Use")}
          </button>
        </div>
        <div className="reef-camera" role="group" aria-label={t("Play camera")}>
          <button
            type="button"
            disabled={!ready}
            onClick={() => controls.current?.zoom(-1)}
            aria-label={t("Zoom out")}
          >
            −
          </button>
          <button
            type="button"
            disabled={!ready}
            onClick={() => controls.current?.zoom(1)}
            aria-label={t("Zoom in")}
          >
            +
          </button>
          <button
            type="button"
            disabled={!ready}
            onClick={() => controls.current?.center()}
            aria-label={t("Centre on me")}
            title={t("Centre on me")}
          >
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden="true"
            >
              <path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5" />
              <path d="M9 9h6v6H9z" />
            </svg>
          </button>
        </div>
        <div className="reef-bottom-hud">
          <p className="reef-hint">{t("Tap a tile to explore")}</p>
          <button
            ref={drawerToggle}
            type="button"
            className="reef-text-button"
            aria-controls="reef-controls"
            aria-expanded={drawer}
            onClick={() => (drawer ? closeDrawer() : openDrawer())}
          >
            {t("Controls")}
          </button>
        </div>
        <p className="reef-status" role="status" aria-live="polite">
          {playerStatus(status)}
        </p>
        {network === "replaced" && (
          <PixelButton type="button" onClick={() => controller.current?.takeOver()}>
            {t("Take over here")}
          </PixelButton>
        )}
        {error && (
          <p className="reef-error" role="alert">
            {t(error)}
          </p>
        )}
      </div>
      <aside
        id="reef-controls"
        className="reef-controls"
        hidden={!drawer}
        aria-label={t("Controls")}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            closeDrawer();
          }
        }}
      >
        <div className="reef-drawer-heading">
          <h2>{t("Controls")}</h2>
          <button ref={drawerClose} type="button" aria-label={t("Close controls")} onClick={closeDrawer}>
            ×
          </button>
        </div>
        {rooms.length > 1 && (
          <label className="reef-room-picker">
            {t("Room")}
            <select value={room.idx} onChange={(e) => onRoom(Number(e.target.value))}>
              {rooms.map((r) => (
                <option key={r.idx} value={r.idx}>
                  {r.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <p id="reef-help" className="muted">
          {t(
            "Tap or click to walk or interact. Drag to pan; pinch or use + / − to zoom. Arrow keys move one tile. Home centres the camera.",
          )}
        </p>
        <form
          className="reef-walk"
          onSubmit={(e) => {
            e.preventDefault();
            controls.current?.move(tile);
          }}
        >
          <label>
            {t("Tile X")}
            <input
              type="number"
              min={0}
              max={room.width - 1}
              value={tile[0]}
              onChange={(e) => setTile([Number(e.target.value), tile[1]])}
            />
          </label>
          <label>
            {t("Tile Y")}
            <input
              type="number"
              min={0}
              max={room.depth - 1}
              value={tile[1]}
              onChange={(e) => setTile([tile[0], Number(e.target.value)])}
            />
          </label>
          <PixelButton type="submit" disabled={!ready}>
            {t("Walk to tile")}
          </PixelButton>
        </form>
        <button type="button" disabled={!ready} onClick={() => controller.current?.send({ type: "stand" })}>
          {t("Stand")}
        </button>
        <h3>{t("Furniture and actions")}</h3>
        <ul className="reef-objects">
          {room.placements.map((p) => {
            const target = targets.find((t) => t.id === p.id);
            return (
              <li key={p.id}>
                <span>
                  {objects.find((o) => o.id === p.id)?.label ?? p.item_id}{" "}
                  <small>
                    ({p.x}, {p.y})
                  </small>
                </span>
                <button
                  type="button"
                  disabled={!ready}
                  onClick={() =>
                    target ? controls.current?.move(target.anchor, target) : controls.current?.walkTo(p.id)
                  }
                >
                  {target?.kind === "seat" ? t("Sit") : target?.kind === "usable" ? t("Use") : t("Walk to")}
                </button>
              </li>
            );
          })}
        </ul>
        {ready && (
          <p className="reef-loading muted">
            {streaming ? "…" : "✓"} {Math.ceil(bytes / 1000)} KB
          </p>
        )}
        {warnings.map((w) => (
          <p key={w} role="status">
            {t(w)}
          </p>
        ))}
      </aside>
      <Suspense
        fallback={
          <p className="reef-loading" role="status">
            {t("Loading…")}
          </p>
        }
      >
        {decorating && editable && (
          <DecoratePanel
            key={`${roomKey}:edit`}
            room={room}
            target={target}
            bridge={editBridge}
            onSaved={onLayout}
            onClose={() => {
              setDecorating(false);
              editBridge.current = null;
              canvas.current?.focus();
            }}
          />
        )}
        {social && (
          <SpacesPanel
            key={roomKey}
            owner={roomKey.split(":")[1]!}
            user={userId}
            plaza={roomKey.startsWith("plaza:")}
            canManage={editable}
            onClose={() => {
              setSocial(false);
              canvas.current?.focus();
            }}
          />
        )}
      </Suspense>
      {chatNet && (
        <ChatPanel
          key={roomKey}
          net={chatNet}
          roomKey={roomKey}
          guildManager={editable && roomKey.startsWith("plaza:")}
        />
      )}
    </section>
  );
}
