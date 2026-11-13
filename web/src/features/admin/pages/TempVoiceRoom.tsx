import { t as deferredText } from "../../../i18n/dashboard";
import { type GuildInfo } from "../../../lib/api";
import { type Creator, type Room, ago } from "../api/tempVoice";
import { t, tf } from "../api/tempVoiceI18n";
import { nameOf } from "./TempVoiceCreator";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { roomSummary } from "./tempVoiceSummary";
// /Rooms

export function RoomRow({
  room,
  info,
  creators,
}: {
  room: Room;
  info: GuildInfo | undefined;
  creators: Creator[];
}) {
  const name = nameOf(info, room.channel_id, t("Voice room"));
  const source = creators.find((c) => c.channel_id === room.source_id);
  const owner =
    room.owner_name ??
    (room.owner_id ? tf(deferredText("Member {id}"), { id: room.owner_id.slice(-4) }) : t("Nobody"));
  const full = room.user_limit > 0 ? `${room.members}/${room.user_limit}` : String(room.members);
  return (
    <li>
      <SettingsRow
        rowId={`room-${room.channel_id}`}
        title={`🔊 ${name}`}
        description={dt("Review room activity.")}
        summary={roomSummary(room)}
      >
        <div className="room">
          <span className="room-owner">
            <span className="pixel-label muted">{t("OWNER")}</span> {owner}
          </span>
          <span className="room-people">
            <span className="pixel-label muted">{t("INSIDE")}</span>{" "}
            <span aria-label={tf(deferredText("{count} people inside"), { count: full })}>{full}</span>
          </span>
          <span className="room-flags">
            {room.locked && <span className="badge badge-gold">{t("LOCKED")}</span>}
            {room.hidden && <span className="badge badge-dim">{t("HIDDEN")}</span>}
            {!source && <span className="badge badge-dim">{t("CREATOR REMOVED")}</span>}
          </span>
          <span className="room-age muted">{ago(room.created_at)}</span>
        </div>
      </SettingsRow>
    </li>
  );
}
