import { PreviewBlock } from "../../../components/PreviewBlock";
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
  const valid = settled === null || checkMessage(kind, settled).ok;
  const preview = useMessagePreview(guild, kind, settled, valid);
  const label = t(kind === "welcome" ? "Welcome preview" : "Goodbye preview");
  return (
    <div aria-live="polite">
      <PreviewBlock label={label} text={preview.data ?? ""} />
    </div>
  );
}
