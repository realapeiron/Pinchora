import { useEffect, useRef, useState, type Dispatch, type RefObject, type SetStateAction } from "react";
import type { RoomJson } from "../../iso/scene";
import { translate as spaTranslate } from "../../i18n";
import { bootstrap } from "../bootstrap";
import type { EditBridge } from "../DecoratePanel";
import type { Controls } from "../StageControls";
import type { Animation } from "../animation";
import { Camera, Gesture } from "../camera";
import { bindStageInput, type SceneInteraction } from "./bindStageInput";
import { startStageFrame, type FramePosition } from "./startStageFrame";
import { createSceneControls } from "./createSceneControls";
import { actorItem, Compositor } from "../compositor";
import { Footsteps } from "../effects";
import { type Target, type WalkTarget } from "../navigation";
import { ReefNet, type NetworkState } from "../net";
import { Player } from "../player";

export interface SceneRuntime {
  el: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  compositor: Compositor;
  camera: Camera;
  gesture: Gesture;
  player: Player;
  net: ReefNet;
  room: RoomJson;
  loaded: Awaited<ReturnType<typeof bootstrap>>;
  interaction: SceneInteraction;
  framePosition: FramePosition;
  motion: MediaQueryList;
  dust: Footsteps;
  actor: ReturnType<typeof actorItem>;
  editBridge: RefObject<EditBridge | null>;
  controls: RefObject<Controls | null>;
  nameTag: RefObject<HTMLDivElement | null>;
  bubble: RefObject<HTMLDivElement | null>;
  abort: AbortController;
  remoteArt: Map<string, Animation>;
  pendingArt: Set<string>;
  tags: Map<string, HTMLDivElement>;
  chatBubbles: Map<string, HTMLButtonElement>;
  setStatus: Dispatch<SetStateAction<string>>;
  setSelected: Dispatch<SetStateAction<Target | null>>;
  setWarnings: Dispatch<SetStateAction<string[]>>;
}

export function useStageScene({
  room,
  own,
  roomKey,
  canvas,
  onLayout,
  controls,
  nameTag,
  bubble,
  editBridge,
  controller,
}: {
  room: RoomJson;
  own: boolean;
  roomKey: string;
  canvas: RefObject<HTMLCanvasElement | null>;
  onLayout: () => void;
  controls: RefObject<Controls | null>;
  nameTag: RefObject<HTMLDivElement | null>;
  bubble: RefObject<HTMLDivElement | null>;
  editBridge: RefObject<EditBridge | null>;
  controller: RefObject<ReefNet | null>;
}) {
  const refreshLayout = useRef(onLayout);
  refreshLayout.current = onLayout;
  const [status, setStatus] = useState(spaTranslate("Loading your character…"));
  const [network, setNetwork] = useState<NetworkState>("preview");
  const [chatNet, setChatNet] = useState<ReefNet | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [targets, setTargets] = useState<Target[]>([]);
  const [objects, setObjects] = useState<WalkTarget[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [streaming, setStreaming] = useState(true);
  const [bytes, setBytes] = useState(0);
  const [selected, setSelected] = useState<Target | null>(null);
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
      const compositor = new Compositor(await loaded.preparation.prepare(loaded.makeScene(), true));
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
      const motion = matchMedia("(prefers-reduced-motion: reduce)");
      const interaction: SceneInteraction = {
        pinch: 0,
        focused: false,
        following: true,
        keepRoomVisible: true,
        centering: true,
        interacted: false,
        contextTarget: null,
      };
      const dust = new Footsteps();
      const framePosition: FramePosition = { openingTime: 0, destination: null, markerAge: 0 };
      const runtime: SceneRuntime = {
        el,
        ctx,
        compositor,
        camera,
        gesture,
        player,
        net,
        room,
        loaded,
        interaction,
        framePosition,
        motion,
        dust,
        actor,
        editBridge,
        controls,
        nameTag,
        bubble,
        abort,
        remoteArt,
        pendingArt,
        tags,
        chatBubbles,
        setStatus,
        setSelected,
        setWarnings,
      };
      const sceneControls = createSceneControls(runtime);
      const removeInput = bindStageInput(runtime, sceneControls);
      const { stopFrame, ghostArt } = startStageFrame(runtime, sceneControls);
      setReady(Boolean(player.tile));
      setTargets(loaded.nav.targets);
      setObjects(loaded.nav.objects);
      setWarnings(
        loaded.warnings.length ? [spaTranslate("Some accessories could not be displayed correctly.")] : [],
      );
      sceneControls.announce();
      loaded.ledger.markPlayable();
      setBytes(loaded.ledger.firstInteraction!);
      el.dataset.ready = "true";
      el.dataset.artReady = "false";
      dispose = () => {
        net.dispose();
        if (controller.current === net) controller.current = null;
        for (const tag of tags.values()) tag.remove();
        for (const node of chatBubbles.values()) node.remove();
        stopFrame();
        sceneControls.observer.disconnect();
        controls.current = null;
        removeInput();
        if (ghostArt.bitmap) ghostArt.bitmap.width = ghostArt.bitmap.height = 0;
        runtime.compositor.dispose();
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
          camera.x += (runtime.compositor.room.origin[0] - next.room.origin[0]) * camera.scale;
          camera.y += (runtime.compositor.room.origin[1] - next.room.origin[1]) * camera.scale;
          runtime.compositor.dispose();
          runtime.compositor = next;
          if (!interaction.interacted) sceneControls.fit();
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
  return { status, network, chatNet, ready, error, targets, objects, warnings, streaming, bytes, selected };
}
