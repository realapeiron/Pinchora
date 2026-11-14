import { useRef, useState } from "react";
import { PixelButton } from "../components/PixelButton";
import type { HouseTarget } from "../features/house/api/house";
import type { RoomJson } from "../iso/scene";
import type { EditBridge } from "./DecoratePanel";
import { StagePanels } from "./stage/StagePanels";
import { CameraControls, RoomControls, type Controls } from "./StageControls";
import { ControlsDrawer } from "./stage/ControlsDrawer";

import { playerStatus, t } from "./i18n";
import { useStageScene } from "./stage/useStageScene";
import type { Point } from "./manifest";
import type { ReefNet } from "./net";
import "./reef.css";

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
  const [activities, setActivities] = useState(false);
  const [gamePlacement, setGamePlacement] = useState<string | null>(null);
  const editBridge = useRef<EditBridge | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const controls = useRef<Controls | null>(null);
  const nameTag = useRef<HTMLDivElement>(null);
  const bubble = useRef<HTMLDivElement>(null);
  const drawerToggle = useRef<HTMLButtonElement>(null);
  const drawerClose = useRef<HTMLButtonElement>(null);
  const [drawer, setDrawer] = useState(() => !matchMedia("(pointer: coarse), (max-width: 767px)").matches);
  const closeDrawer = () => {
    setDrawer(false);
    drawerToggle.current?.focus({ preventScroll: true });
  };
  const openDrawer = () => {
    setDrawer(true);
    requestAnimationFrame(() => drawerClose.current?.focus({ preventScroll: true }));
  };
  const controller = useRef<ReefNet | null>(null);
  const [tile, setTile] = useState<Point>([0, 0]);
  const { status, network, chatNet, ready, error, targets, objects, warnings, streaming, bytes, selected } =
    useStageScene({
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
    });
  const onUsePlacement = (id: string) => {
    setGamePlacement(id);
    setActivities(true);
    setDrawer(false);
    setSocial(false);
    setDecorating(false);
  };
  return (
    <section
      className="reef"
      aria-label={t("Play in this room")}
      data-controls-open={drawer}
      data-decorating={decorating}
      data-panel-open={decorating || social || activities || drawer}
    >
      <div className="reef-surface">
        <canvas
          ref={canvas}
          className="reef-canvas"
          data-network={network}
          tabIndex={0}
          /* eslint-disable-next-line jsx-a11y/no-interactive-element-to-noninteractive-role -- The focusable canvas handles the room keyboard controls. */
          role="application"
          aria-label={t("Playable room. Arrow keys move one tile. Home centres the camera.")}
          aria-describedby="reef-help"
        />
        <div className="reef-vignette" aria-hidden="true" />
        <RoomControls
          {...{
            roomName,
            editable,
            network,
            decorating,
            social,
            activities,
            setActivities,
            setDecorating,
            setSocial,
            setDrawer,
            setGamePlacement,
            onExit,
          }}
        />
        <div className="reef-name-tag" ref={nameTag} hidden={!ready}>
          {t("You")}
        </div>
        <div className="reef-context" ref={bubble} hidden={!selected}>
          <button
            type="button"
            onClick={() => {
              if (selected?.kind === "usable") {
                onUsePlacement(selected.id);
              }
              if (selected) controls.current?.move(selected.anchor, selected);
              if (selected?.kind !== "usable") canvas.current?.focus({ preventScroll: true });
            }}
          >
            {selected?.kind === "seat" ? t("Sit") : t("Use")}
          </button>
        </div>
        <CameraControls ready={ready} controls={controls} />
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
      <ControlsDrawer
        drawer={drawer}
        closeDrawer={closeDrawer}
        drawerClose={drawerClose}
        rooms={rooms}
        room={room}
        onRoom={onRoom}
        ready={ready}
        controls={controls}
        tile={tile}
        setTile={setTile}
        controller={controller}
        targets={targets}
        objects={objects}
        onUsePlacement={onUsePlacement}
        streaming={streaming}
        bytes={bytes}
        warnings={warnings}
      />
      <StagePanels
        decorating={decorating}
        editable={editable}
        roomKey={roomKey}
        room={room}
        target={target}
        editBridge={editBridge}
        onLayout={onLayout}
        setDecorating={setDecorating}
        canvas={canvas}
        activities={activities}
        userId={userId}
        gamePlacement={gamePlacement}
        setActivities={setActivities}
        social={social}
        setSocial={setSocial}
        chatNet={chatNet}
      />
    </section>
  );
}
