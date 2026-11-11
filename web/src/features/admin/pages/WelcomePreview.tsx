import { PreviewBlock } from "../../../components/PreviewBlock";
import { translate as spaTranslate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { useMessagePreview } from "../../../lib/queries";
import { checkMessage, type MessageKind } from "../../../lib/rules";
import { useDebounced } from "../../../lib/useDebounced";

export function WelcomePreview({
  guild,
  kind,
  message,
}: {
  guild: string;
  kind: MessageKind;
  message: string | null;
}) {
  const settled = useDebounced(message, 350);
  const valid = message === null || checkMessage(kind, message).ok;
  const preview = useMessagePreview(guild, kind, settled, valid);
  const label = t(kind === "welcome" ? "Welcome preview" : "Goodbye preview");
  const status = !valid
    ? spaTranslate("Fix the message to see it.")
    : message !== settled || preview.isPending
      ? "…"
      : preview.isError
        ? spaTranslate("Preview unavailable. Check your choices and try again.")
        : undefined;
  return (
    <div aria-live="polite">
      <PreviewBlock label={label} text={preview.data} status={status} />
    </div>
  );
}
