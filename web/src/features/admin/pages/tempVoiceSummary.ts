import type { Creator, Room, TempVoice } from "../api/tempVoice";
import { t } from "../../../i18n/dashboard";

export function tempVoiceSummary(data: TempVoice) {
  return {
    creators: data.creators.length
      ? t(data.creators.length === 1 ? "{count} creator channel" : "{count} creator channels", {
          count: data.creators.length,
        })
      : t("No creator channels yet"),
    rooms: data.rooms.length
      ? t(data.rooms.length === 1 ? "{count} live room" : "{count} live rooms", { count: data.rooms.length })
      : t("No rooms right now"),
    controls: t("{count} member commands", { count: 10 }),
  };
}

export function creatorSummary(creator: Creator) {
  return creator.enabled
    ? `${creator.name_template} · ${creator.default_limit || t("No people limit")} · ${creator.rooms} ${t("open now")}`
    : t("Off");
}

export function roomSummary(room: Room) {
  return `${room.owner_name ?? t("Unknown owner")} · ${room.members}${room.user_limit ? `/${room.user_limit}` : ""} ${t("inside")}`;
}
