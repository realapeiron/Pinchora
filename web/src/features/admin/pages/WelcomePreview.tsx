import { t as deferredText } from "../../../i18n/dashboard";
import { useState } from "react";
import { PreviewBlock } from "../../../components/PreviewBlock";
import { PixelButton } from "../../../components/PixelButton";
import { Toast } from "../../../components/Toast";
import { translate as spaTranslate } from "../../../i18n";
import { t } from "../../../i18n/dashboard";
import { useMessagePreview } from "../../../lib/queries";
import { checkMessage, type MessageKind } from "../../../lib/rules";
import { useDebounced } from "../../../lib/useDebounced";
import { api } from "../../../lib/api";

export function WelcomePreview({
  guild,
  kind,
  message,
  channel,
  channelName,
  dm = false,
  showTest = true,
}: {
  guild: string;
  kind: MessageKind;
  message: string | null;
  channel: string | null;
  channelName?: string;
  dm?: boolean;
  showTest?: boolean;
}) {
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const settled = useDebounced(message, 350);
  const valid = message === null || checkMessage(kind, message).ok;
  const preview = useMessagePreview(guild, kind, settled, valid);
  const label = t(dm ? "Welcome DM preview" : kind === "welcome" ? "Welcome preview" : "Goodbye preview");
  const send = async () => {
    setSending(true);
    try {
      await api(`/api/guilds/${guild}/messages/${dm ? "welcome_dm" : kind}/test`, "POST", {
        message,
        channel_id: channel,
      });
      setNotice(
        t(
          dm
            ? "Test message sent to your DMs; it shows you as a new member."
            : "Test message sent to #{channel}; it shows you as a new member.",
          {
            channel: channelName ?? channel ?? "",
          },
        ),
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : t("Test message could not be sent."));
    } finally {
      setSending(false);
    }
  };
  const status = !valid
    ? deferredText("Fix the message to see it.")
    : message !== settled || preview.isPending
      ? "…"
      : preview.isError
        ? spaTranslate("Preview unavailable. Check your choices and try again.")
        : undefined;
  return (
    <div aria-live="polite">
      <PreviewBlock
        label={label}
        text={preview.data}
        status={status}
        action={
          showTest && (
            <PixelButton
              className="message-preview-test"
              variant="ghost"
              disabled={sending || !valid || (!dm && !channel)}
              onClick={() => void send()}
            >
              {t(sending ? "Sending…" : "Send test")}
            </PixelButton>
          )
        }
      />
      <Toast message={notice} onDone={() => setNotice(null)} />
    </div>
  );
}
