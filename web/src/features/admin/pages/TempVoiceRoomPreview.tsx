import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { roomName, type Creator } from "../api/tempVoice";
import { t, tf } from "../api/tempVoiceI18n";

const SAMPLE_OWNER = "Mira";
export function RoomPreview({
  value,
  rooms,
  templateError,
  previewChat,
}: {
  value: Creator;
  rooms: number;
  templateError: string | null;
  previewChat: string;
}) {
  return (
    <div className="creator-preview settings-detail-preview" role="region" aria-label={t("New room preview")}>
      <strong>
        🔊{" "}
        {templateError
          ? t("Fix the room name above")
          : roomName(value.name_template, SAMPLE_OWNER, rooms + 1)}
      </strong>
      <span>
        {t(value.default_locked ? deferredText("Starts locked") : deferredText("Starts open"))} ·{" "}
        {value.default_limit
          ? tf(spaTranslate("{count} people"), { count: value.default_limit })
          : t("No people limit")}
      </span>
      <span>{previewChat}</span>
    </div>
  );
}
