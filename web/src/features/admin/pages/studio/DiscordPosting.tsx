import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { ChannelPicker } from "../../../../components/DashboardPickers";

export function DiscordPosting({
  discord,
  channel,
  setChannel,
  messageId,
  setMessageId,
  kitKey,
  dirty,
  busy,
  preview,
  send,
}: Pick<
  StudioModel,
  "discord" | "channel" | "setChannel" | "messageId" | "setMessageId" | "dirty" | "busy" | "preview" | "send"
> & { kitKey: StudioModel["key"] }) {
  const key = kitKey;
  return (
    <Panel title={deferredText("POST OR EDIT IN DISCORD")}>
      <div className="studio-actions">
        <ChannelPicker
          posting
          label={t("Channel")}
          info={discord.data}
          value={channel}
          onChange={setChannel}
        />
        <input
          className="pixel-input"
          aria-label={deferredText("Existing message ID to edit (optional)")}
          value={messageId}
          onChange={(event) => setMessageId(event.target.value)}
          placeholder={deferredText("Message ID to edit (optional)")}
        />
        <PixelButton disabled={!channel || !key || dirty || busy || !preview} onClick={() => void send()}>
          {messageId ? deferredText("Update message") : deferredText("Send message")}
        </PixelButton>
      </div>
      <p className="muted">
        {deferredText("Save the kit before posting. Enter a Pinchora message ID to edit it.")}
      </p>
    </Panel>
  );
}
