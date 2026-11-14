import { PixelButton } from "../../../../components/PixelButton";
import { t } from "../../../../reef/i18n";
import { roomLabel, type HouseTarget, type RoomSummary } from "../../api/house";

type Props = {
  targetKind: HouseTarget["kind"];
  playEnabled: boolean;
  selectRoom: (nextRoom: number) => void;
  selectScope: (scope: string) => void;
  selectGuild: (guildId: string) => void;
  toggleEdit: () => void;
  selectedGuild: string;
  guilds: { id: string; name: string; bot_present: boolean }[] | undefined;
  phone: boolean;
  editable: boolean;
  canDecorate: boolean;
  editing: boolean;
  rooms: Pick<RoomSummary, "idx" | "room_type" | "name" | "items">[];
  idx: number | null;
  selectMode: (mode: "view" | "play") => void;
};

export function Navigation({
  targetKind,
  playEnabled,
  selectRoom,
  selectScope,
  selectGuild,
  toggleEdit,
  selectedGuild,
  guilds,
  phone,
  editable,
  canDecorate,
  editing,
  rooms,
  idx,
  selectMode,
}: Props) {
  return (
    <>
      {targetKind !== "visit" && (
        <div className="house-options">
          <label>
            {t("House scope")}
            <select value={targetKind} onChange={(e) => selectScope(e.target.value)}>
              <option value="own">{t("My house")}</option>
              <option value="partner">{t("Partner’s house")}</option>
              <option value="plaza" disabled={!/^\d{15,21}$/.test(selectedGuild)}>
                {t("Server plaza")}
              </option>
            </select>
          </label>
          <label>
            {t("Server plaza")}
            <select value={selectedGuild} onChange={(e) => selectGuild(e.target.value)}>
              <option value="">{t("Choose a server")}</option>
              {guilds
                ?.filter((g) => g.bot_present)
                .map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.name}
                  </option>
                ))}
            </select>
          </label>
          {!phone && editable && (
            <PixelButton disabled={!canDecorate} aria-pressed={editing} onClick={toggleEdit}>
              {editing ? t("View mode") : t("Decorate house")}
            </PixelButton>
          )}
          {phone && <span className="muted">{t("View mode on phones. Decorate on a larger screen.")}</span>}
        </div>
      )}
      {rooms.length > 1 && (
        <div className="house-tabs" role="tablist" aria-label={t("Rooms")}>
          {rooms.map((r) => (
            <PixelButton
              key={r.idx}
              role="tab"
              aria-selected={r.idx === idx}
              variant={r.idx === idx ? "primary" : "secondary"}
              onClick={() => selectRoom(r.idx)}
            >
              {roomLabel(r.room_type, r.name)} <span className="house-tab-count">{r.items}</span>
            </PixelButton>
          ))}
        </div>
      )}
      {playEnabled && (
        <div className="house-options" role="group" aria-label={t("Room mode")}>
          <PixelButton aria-pressed={false} onClick={() => selectMode("play")}>
            {t("Play")}
          </PixelButton>
          <PixelButton variant="secondary" aria-pressed={true} onClick={() => selectMode("view")}>
            {t("View")}
          </PixelButton>
        </div>
      )}
    </>
  );
}
