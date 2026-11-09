import type { Dispatch, RefObject, SetStateAction } from "react";
import { translate as spaTranslate } from "../i18n";
import type { SceneItem } from "../iso/scene";
import { poseAt, type Animation } from "./animation";
import type { bootstrap } from "./bootstrap";
import type { Compositor } from "./compositor";
import { actorItem, type ActorDraw } from "./compositor";
import { t } from "./i18n";
import type { Point } from "./manifest";
import type { Target } from "./navigation";
import type { ReefNet } from "./net";
import type { Player } from "./player";

type PlaceOverlay = (node: HTMLElement | null, point: Point, lift: number) => void;
interface RemotePlayers {
  net: ReefNet;
  loaded: Awaited<ReturnType<typeof bootstrap>>;
  abort: AbortController;
  motion: MediaQueryList;
  remoteArt: Map<string, Animation>;
  pendingArt: Set<string>;
  tags: Map<string, HTMLDivElement>;
  el: HTMLCanvasElement;
  setWarnings: Dispatch<SetStateAction<string[]>>;
  seatMask: (id: string | undefined) => ActorDraw["seat"];
  placeOverlay: PlaceOverlay;
}
export function drawRemotePlayers(
  {
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
  }: RemotePlayers,
  now: number,
) {
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
  return remoteDraws;
}
export function drawChatBubbles(
  net: ReefNet,
  chatBubbles: Map<string, HTMLButtonElement>,
  el: HTMLCanvasElement,
  placeOverlay: PlaceOverlay,
  now: number,
) {
  const latest = new Map(
    net.chat
      .snapshot()
      .filter((line) => line.delivery === "delivered" && Date.now() - Date.parse(line.created_at) < 8000)
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
}

interface LocalPlayer {
  player: Player;
  actor: SceneItem;
  net: ReefNet;
  loaded: Awaited<ReturnType<typeof bootstrap>>;
  motion: MediaQueryList;
  remoteDraws: ActorDraw[];
  ground: () => void;
  nameTag: RefObject<HTMLDivElement | null>;
  bubble: RefObject<HTMLDivElement | null>;
  contextTarget: Target | null;
  focused: boolean;
  placeOverlay: PlaceOverlay;
  seatMask: (id: string | undefined) => ActorDraw["seat"];
}
export function drawLocalPlayer(
  {
    player,
    actor,
    net,
    loaded,
    motion,
    remoteDraws,
    ground,
    nameTag,
    bubble,
    contextTarget,
    focused,
    placeOverlay,
    seatMask,
  }: LocalPlayer,
  ctx: CanvasRenderingContext2D,
  compositor: Compositor,
) {
  const pos = player.state === "sit" && player.target ? player.target.anchor : player.position;
  if (pos) {
    actor.info.label = net.userId;
    actor.x = pos[0];
    actor.y = pos[1];
    const f = loaded.animation.get(player.direction, poseAt(player.state, player.elapsed, motion.matches));
    const lift =
      player.state === "sit" && player.target
        ? 122 - player.target.height - (f.meta.seat_contact?.[1] ?? 101)
        : 8;
    const bob = !motion.matches && player.state === "idle" ? Math.round(Math.sin(player.elapsed / 450)) : 0;
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
}
