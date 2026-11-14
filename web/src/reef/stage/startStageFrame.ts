import { drawGhost, prepareGhost, type GhostArt } from "../ghostDrawing";
import { drawChatBubbles, drawLocalPlayer, drawRemotePlayers } from "../playerOverlays";
import { drawGround } from "../groundDrawing";
import { SEAT_FOREGROUND_ROWS } from "../seats";
import type { Point } from "../manifest";
import type { SceneControls } from "./createSceneControls";
import type { SceneRuntime } from "./useStageScene";

export interface FramePosition {
  openingTime: number;
  destination: Point | null;
  markerAge: number;
}

export function startStageFrame(runtime: SceneRuntime, sceneControls: SceneControls) {
  const {
    el,
    ctx,
    camera,
    player,
    net,
    room,
    motion,
    dust,
    interaction,
    framePosition,
    editBridge,
    chatBubbles,
    nameTag,
    bubble,
    loaded,
    actor,
    abort,
    remoteArt,
    pendingArt,
    tags,
    setWarnings,
  } = runtime;
  const { announce, size, playerPoint, roomSize } = sceneControls;
  const seatMask = (id: string | undefined) => {
    const piece = room.placements.find((p) => p.id === id);
    const base = piece ? loaded.content.item(piece.item_id)?.base.id : undefined;
    return id && base && SEAT_FOREGROUND_ROWS[base] ? { id, rows: SEAT_FOREGROUND_ROWS[base]! } : undefined;
  };
  const ground = () =>
    drawGround(
      ctx,
      runtime.compositor.room.origin,
      framePosition.destination,
      framePosition.markerAge,
      motion.matches,
      dust,
    );
  const placeOverlay = (node: HTMLElement | null, point: Point, lift: number) => {
    if (!node) return;
    const [ox, oy] = runtime.compositor.room.origin;
    const x = (camera.x + (ox + 32 * (point[0] - point[1])) * camera.scale) / camera.dpr;
    const y = (camera.y + (oy + 16 * (point[0] + point[1]) - lift) * camera.scale) / camera.dpr;
    node.style.left = `${x}px`;
    node.style.top = `${y}px`;
  };
  const ghostArt: GhostArt = {
    id: "",
    art: null,
    bitmap: null,
    orientation: "",
    position: "",
    item: null,
  };
  const overlays = {
    player,
    actor,
    ground,
    nameTag,
    bubble,
    net,
    loaded,
    abort,
    motion,
    remoteArt,
    pendingArt,
    tags,
    el,
    setWarnings,
    seatMask,
    placeOverlay,
  };
  const ghostPreparation = { loaded, abort, room, setWarnings };
  let raf = 0,
    last = 0,
    ticks = 0;
  const frames: number[] = [];
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
    const remoteDraws = drawRemotePlayers(overlays, now);
    announce();
    framePosition.openingTime += dt;
    framePosition.markerAge += dt;
    dust.update(dt, player.state === "walk", motion.matches, player.position);
    if (interaction.following) {
      if (framePosition.openingTime > 700 || motion.matches) {
        camera.follow(
          ...size(),
          playerPoint(),
          roomSize(),
          dt,
          motion.matches,
          interaction.centering ? 0 : 24,
          interaction.keepRoomVisible,
        );
        if (framePosition.openingTime > 1800) interaction.centering = false;
      }
    }
    el.dataset.origin = runtime.compositor.room.origin.join(",");
    el.dataset.roomSize = roomSize().join(",");
    el.dataset.scale = String(camera.scale);
    el.dataset.camera = `${camera.x},${camera.y}`;
    el.dataset.cameraSettled = String(framePosition.openingTime > 1800);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, el.width, el.height);
    ctx.setTransform(camera.scale, 0, 0, camera.scale, camera.x, camera.y);
    drawLocalPlayer(
      overlays,
      remoteDraws,
      interaction.contextTarget,
      interaction.focused,
      ctx,
      runtime.compositor,
    );
    const ghost = editBridge.current?.ghost;
    if (ghost) {
      prepareGhost(ghostArt, ghost, ghostPreparation);
      drawGhost(ghostArt, ghost, ctx, runtime.compositor.room.origin);
    }
    drawChatBubbles(net, chatBubbles, el, placeOverlay, now);
    if (nameTag.current && net.ready) nameTag.current.textContent = net.players.get(net.userId)?.name ?? "🐚";
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
  return { stopFrame: () => cancelAnimationFrame(raf), ghostArt };
}
