import type { Dispatch, RefObject, SetStateAction } from "react";
import { PixelButton } from "../components/PixelButton";
import { PageHead } from "../features/shell/pages/PageHead";
import type { RoomJson } from "../iso/scene";
import { t } from "./i18n";
import type { Point } from "./manifest";
import type { Target } from "./navigation";
import type { NetworkState } from "./net";

export interface Controls {
  walkTo: (id: string) => void;
  move: (p: Point, target?: Target) => void;
  zoom: (delta: number) => void;
  center: () => void;
}
interface CameraProps {
  ready: boolean;
  controls: RefObject<Controls | null>;
}
export function CameraControls({ ready, controls }: CameraProps) {
  return (
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
  );
}
interface TileProps extends CameraProps {
  room: RoomJson;
  tile: Point;
  setTile: Dispatch<SetStateAction<Point>>;
}
export function TileControls({ ready, controls, room, tile, setTile }: TileProps) {
  return (
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
  );
}

interface RoomControlsProps {
  roomName: string;
  editable: boolean;
  network: NetworkState;
  decorating: boolean;
  social: boolean;
  activities: boolean;
  setActivities: (on: boolean) => void;
  setDecorating: (on: boolean) => void;
  setSocial: (on: boolean) => void;
  setDrawer: (on: boolean) => void;
  setGamePlacement: (id: string | null) => void;
  onExit: () => void;
}
export function RoomControls({
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
}: RoomControlsProps) {
  return (
    <div className="reef-room-hud">
      <div className="reef-room-name">
        <PageHead title={roomName} lead={null} />
      </div>
      {editable && (
        <button
          type="button"
          onClick={() => {
            setActivities(false);
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
          setActivities(false);
          setSocial(!social);
          setDecorating(false);
          setDrawer(false);
        }}
      >
        {t("Social spaces")}
      </button>
      <button
        type="button"
        disabled={network !== "connected"}
        onClick={() => {
          setActivities(!activities);
          setGamePlacement(null);
          setDecorating(false);
          setSocial(false);
          setDrawer(false);
        }}
      >
        {t("Room activities")}
      </button>
      <button type="button" className="reef-text-button" onClick={onExit}>
        {t("View")}
      </button>
    </div>
  );
}
