import type { Dispatch, RefObject, SetStateAction } from "react";
import type { RoomJson } from "../../iso/scene";
import { t } from "../i18n";
import type { Point } from "../manifest";
import type { Target, WalkTarget } from "../navigation";
import type { ReefNet } from "../net";
import { TileControls, type Controls } from "../StageControls";

export function ControlsDrawer({
  drawer,
  closeDrawer,
  drawerClose,
  rooms,
  room,
  onRoom,
  ready,
  controls,
  tile,
  setTile,
  controller,
  targets,
  objects,
  onUsePlacement,
  streaming,
  bytes,
  warnings,
}: {
  drawer: boolean;
  closeDrawer: () => void;
  drawerClose: RefObject<HTMLButtonElement | null>;
  rooms: { idx: number; name: string }[];
  room: RoomJson;
  onRoom: (idx: number) => void;
  ready: boolean;
  controls: RefObject<Controls | null>;
  tile: Point;
  setTile: Dispatch<SetStateAction<Point>>;
  controller: RefObject<ReefNet | null>;
  targets: Target[];
  objects: WalkTarget[];
  onUsePlacement: (id: string) => void;
  streaming: boolean;
  bytes: number;
  warnings: string[];
}) {
  return (
    <>
      {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Escape closes the drawer from its focused controls. */}
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
        <p id="reef-help" className="muted">
          {t(
            "Tap or click to walk or interact. Drag to pan; pinch or use + / − to zoom. Arrow keys move one tile. Home centres the camera.",
          )}
        </p>
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
        <TileControls ready={ready} controls={controls} room={room} tile={tile} setTile={setTile} />
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
                  onClick={() => {
                    if (target?.kind === "usable") {
                      onUsePlacement(p.id);
                    }
                    if (target) controls.current?.move(target.anchor, target);
                    else controls.current?.walkTo(p.id);
                  }}
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
    </>
  );
}
