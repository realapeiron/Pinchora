import { lazy, Suspense, type Dispatch, type RefObject, type SetStateAction } from "react";
import type { HouseTarget } from "../../features/house/api/house";
import type { RoomJson } from "../../iso/scene";
import { ChatPanel } from "../ChatPanel";
import type { EditBridge } from "../DecoratePanel";
import { t } from "../i18n";
import type { ReefNet } from "../net";

const ActivityPanel = lazy(() => import("../games/ActivityPanel"));
const DecoratePanel = lazy(() => import("../DecoratePanel").then((m) => ({ default: m.DecoratePanel })));
const SpacesPanel = lazy(() => import("../SpacesPanel").then((m) => ({ default: m.SpacesPanel })));

export function StagePanels({
  decorating,
  editable,
  roomKey,
  room,
  target,
  editBridge,
  onLayout,
  setDecorating,
  canvas,
  activities,
  userId,
  gamePlacement,
  setActivities,
  social,
  setSocial,
  chatNet,
}: {
  decorating: boolean;
  editable: boolean;
  roomKey: string;
  room: RoomJson;
  target: HouseTarget;
  editBridge: RefObject<EditBridge | null>;
  onLayout: () => void;
  setDecorating: Dispatch<SetStateAction<boolean>>;
  canvas: RefObject<HTMLCanvasElement | null>;
  activities: boolean;
  userId: string;
  gamePlacement: string | null;
  setActivities: Dispatch<SetStateAction<boolean>>;
  social: boolean;
  setSocial: Dispatch<SetStateAction<boolean>>;
  chatNet: ReefNet | null;
}) {
  return (
    <>
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
        {activities && (
          <ActivityPanel
            room={roomKey}
            user={userId}
            placement={gamePlacement}
            bubbleTarget={canvas.current?.parentElement ?? null}
            onClose={() => {
              setActivities(false);
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
      <Suspense fallback={null}>
        {chatNet && (
          <ChatPanel
            key={roomKey}
            net={chatNet}
            roomKey={roomKey}
            guildManager={editable && roomKey.startsWith("plaza:")}
          />
        )}
      </Suspense>
    </>
  );
}
