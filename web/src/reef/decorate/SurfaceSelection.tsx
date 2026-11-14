import type { Dispatch, SetStateAction } from "react";
import type { RoomJson } from "../../iso/scene";
import { locale } from "../../i18n";
import type { EditOp } from "../../features/house/api/houseEdit";
import { t } from "../i18n";
import type { Surface } from "./useDecorateRoom";

export function SurfaceSelection({
  room,
  surface,
  setSurface,
  surfaces,
  pending,
  send,
}: {
  room: Pick<RoomJson, "wallpaper" | "floor">;
  surface: string;
  setSurface: Dispatch<SetStateAction<string>>;
  surfaces: { wallpapers: Surface[]; floors: Surface[] } | undefined;
  pending: boolean;
  send: (op: EditOp) => void;
}) {
  return (
    <>
      <label>
        {t("Surfaces")}
        <select value={surface} onChange={(e) => setSurface(e.target.value)}>
          <option value="">{t("Choose a surface")}</option>
          {(["wallpapers", "floors"] as const).flatMap(
            (kind) =>
              surfaces?.[kind]
                .filter((s) => s.owned)
                .map((s) => (
                  <option key={`${kind}:${s.id}`} value={`${kind}:${s.id}`}>
                    {locale() === "tr" ? (s.name_tr ?? s.name) : s.name}
                  </option>
                )) ?? [],
          )}
        </select>
      </label>
      <button
        type="button"
        disabled={
          !surface ||
          pending ||
          surface === `wallpapers:${room.wallpaper}` ||
          surface === `floors:${room.floor}`
        }
        onClick={() => {
          const [k, id] = surface.split(":");
          send(k === "wallpapers" ? { type: "surface", wallpaper: id } : { type: "surface", floor: id });
        }}
      >
        {t("Apply surface")}
      </button>
    </>
  );
}
