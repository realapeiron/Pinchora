import { t as deferredText } from "../../../i18n/dashboard";
import { translate } from "../../../i18n";
import { useMessagePreview } from "../../../lib/queries";
import { checkMessage } from "../../../lib/rules";
import { useDebounced } from "../../../lib/useDebounced";
import { RichText } from "../../../components/RichText";

export function LevelUpPreview({ guild, message }: { guild: string; message: string | null }) {
  const settled = useDebounced(message, 350);
  const valid = settled === null || checkMessage("levelup", settled).ok;
  const preview = useMessagePreview(guild, "levelup", settled, valid);
  return (
    <div className="settings-detail-preview message-preview" aria-live="polite">
      <span className="pixel-label muted">{translate("PREVIEW")}</span>
      <p>
        {preview.data !== undefined ? (
          <RichText text={preview.data} />
        ) : (
          <span className="muted">{valid ? "…" : deferredText("Fix the message to see it.")}</span>
        )}
      </p>
    </div>
  );
}
