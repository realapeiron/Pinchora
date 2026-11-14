import { playerStatus } from "../i18n";
import type { Point } from "../manifest";
import { findPath, type Target } from "../navigation";
import type { SceneRuntime } from "./useStageScene";

export function createSceneControls(runtime: SceneRuntime) {
  const { el, camera, player, interaction, framePosition, net, loaded, setStatus, setSelected, controls } =
    runtime;
  let announced = "";
  const size = (): Point => [el.width / camera.dpr, el.height / camera.dpr];
  const roomSize = (): Point => [runtime.compositor.background.width, runtime.compositor.background.height];
  const playerPoint = (): Point => {
    const p = player.state === "sit" && player.target ? player.target.anchor : player.position;
    const [ox, oy] = runtime.compositor.room.origin;
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
    interaction.keepRoomVisible = false;
    interaction.following = true;
    interaction.centering = true;
    framePosition.openingTime = 1000;
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
    interaction.keepRoomVisible = true;
    interaction.following = true;
    interaction.centering = true;
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
    interaction.interacted = true;
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
      framePosition.destination = p;
      framePosition.markerAge = 0;
      interaction.following = true;
      interaction.keepRoomVisible = false;
      interaction.centering = false;
      framePosition.openingTime = 1801;
    }
    interaction.contextTarget = null;
    setSelected(null);
    announce();
  };
  const zoom = (delta: number) => {
    interaction.interacted = true;
    interaction.centering = false;
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
  return { size, roomSize, playerPoint, fit, center, observer, announce, move, walkTo };
}

export type SceneControls = ReturnType<typeof createSceneControls>;
