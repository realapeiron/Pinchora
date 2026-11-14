import { levelTop, WALL_LEVELS } from "../../iso/projection";
import type { Point } from "../manifest";
import type { Target } from "../navigation";
import type { SceneControls } from "./createSceneControls";
import type { SceneRuntime } from "./useStageScene";

export interface SceneInteraction {
  pinch: number;
  focused: boolean;
  following: boolean;
  keepRoomVisible: boolean;
  centering: boolean;
  interacted: boolean;
  contextTarget: Target | null;
}

export function bindStageInput(runtime: SceneRuntime, { move, walkTo, center }: SceneControls) {
  const { el, camera, gesture, editBridge, loaded, room, player, interaction, setSelected } = runtime;
  const local = (e: PointerEvent): Point => {
    const r = el.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const placementTile = (point: Point): Point | null => {
    const ghost = editBridge.current?.ghost;
    if (ghost && loaded.content.item(ghost.item_id)?.base.layer === "wall") {
      const [sx, sy] = camera.scene(...point);
      const x = sx - runtime.compositor.room.origin[0],
        y = sy - runtime.compositor.room.origin[1];
      if (x < 0 !== ghost.flipped) return null;
      const tile = Math.floor(Math.abs(x) / 32),
        level = Math.floor((y - levelTop(x)) / 32);
      return tile < (ghost.flipped ? room.depth : room.width) && level >= 0 && level < WALL_LEVELS
        ? [tile, level]
        : null;
    }
    return camera.tile(...point, runtime.compositor.room.origin, room.width, room.depth);
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
      interaction.interacted = true;
      interaction.following = false;
      interaction.centering = false;
    }
    camera.pan(...result.pan);
    interaction.pinch += result.pinch;
    if (Math.abs(interaction.pinch) >= 40) {
      camera.zoom(camera.scale + Math.sign(interaction.pinch), local(e));
      interaction.pinch = 0;
    }
  };
  const up = (e: PointerEvent) => {
    if (!gesture.up(e.pointerId)) return;
    const p = local(e);
    const [x, y] = camera.scene(...p);
    const index = runtime.compositor.pick(x, y);
    const item = index === null ? null : runtime.compositor.room.scene.items[index];
    if (editBridge.current) {
      editBridge.current.pick(placementTile(p), item?.info.placement ?? null, false);
      return;
    }
    const target = loaded.nav.targets.find((t) => t.id === item?.info.placement);
    if (target) {
      interaction.contextTarget = target;
      setSelected(target);
    } else if (item?.info.placement) walkTo(item.info.placement);
    else {
      const tile = camera.tile(...p, runtime.compositor.room.origin, room.width, room.depth);
      if (tile) move(tile);
    }
  };
  const cancel = () => {
    gesture.cancel();
    interaction.pinch = 0;
  };
  const wheel = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      interaction.interacted = true;
      interaction.following = false;
      interaction.centering = false;
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
      interaction.contextTarget = null;
      setSelected(null);
    }
    if (e.key === "Home") {
      e.preventDefault();
      center();
    }
  };
  const focus = () => {
    interaction.focused = true;
  };
  const blur = () => {
    interaction.focused = false;
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
  return () => {
    el.removeEventListener("pointerdown", down);
    el.removeEventListener("pointermove", drag);
    el.removeEventListener("pointerup", up);
    el.removeEventListener("pointercancel", cancel);
    el.removeEventListener("lostpointercapture", cancel);
    el.removeEventListener("wheel", wheel);
    el.removeEventListener("keydown", keydown);
    el.removeEventListener("focus", focus);
    el.removeEventListener("blur", blur);
  };
}
