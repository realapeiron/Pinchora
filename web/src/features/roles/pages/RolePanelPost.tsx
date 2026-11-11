import { translate as spaTranslate } from "../../../i18n";
import { useState } from "react";
import { Field } from "../../../components/Field";
import { ChannelPicker, ChannelTag } from "../../../components/Pickers";
import { PixelButton } from "../../../components/PixelButton";
import { RichText } from "../../../components/RichText";
import { ApiError, type GuildInfo } from "../../../lib/api";
import { type Panel as PanelData, useRolesWrite } from "../api/roles";

export function PostRow({
  info,
  panel,
  unsaved,
  posting,
  blocked,
}: {
  info: GuildInfo | undefined;
  panel: PanelData;
  unsaved: boolean;
  posting: ReturnType<typeof useRolesWrite>;
  blocked: boolean;
}) {
  const [channel, setChannel] = useState<string | null>(panel.channel_id);
  const error = posting.error instanceof ApiError ? posting.error : null;
  const sameSpot = !!panel.message_id && channel === panel.channel_id;
  const posted = posting.data?.posted;
  return (
    <div>
      <div className="post-row">
        <Field
          label={spaTranslate("Post it in")}
          htmlFor="panel-channel"
          error={error?.field === "channel_id" ? error.message : null}
          hint={
            unsaved
              ? spaTranslate("Save your changes first: the bot posts what is saved.")
              : sameSpot
                ? spaTranslate("The panel's message is edited in place.")
                : spaTranslate("Text or announcement channels the bot can see.")
          }
        >
          <ChannelPicker
            id="panel-channel"
            info={info}
            value={channel}
            onChange={setChannel}
            none={spaTranslate("Pick a channel")}
          />
        </Field>
        <div className="add-actions">
          <PixelButton
            disabled={!channel || unsaved || blocked || posting.isPending}
            onClick={() =>
              channel &&
              posting.mutate({
                method: "POST",
                sub: `/panels/${panel.id}/post`,
                body: { channel_id: channel },
              })
            }
          >
            {posting.isPending
              ? spaTranslate("Posting…")
              : sameSpot
                ? spaTranslate("Update message")
                : spaTranslate("Post panel")}
          </PixelButton>
        </div>
      </div>
      {error && error.field !== "channel_id" && !unsaved && posting.variables?.sub.endsWith("/post") && (
        <p className="field-error" role="alert">
          <RichText text={error.message} />
        </p>
      )}
      {posted && posting.variables?.sub === `/panels/${panel.id}/post` && (
        <p className="notice" role="status">
          {posted.updated ? spaTranslate("Updated") : spaTranslate("Posted")} {spaTranslate("in")}{" "}
          <ChannelTag info={info} id={posted.channel_id} />:{" "}
          <a href={posted.url} target="_blank" rel="noopener noreferrer">
            {" "}
            {spaTranslate("open the message")}{" "}
          </a>
          {posted.unusable.length > 0 && (
            <>
              {" "}
              {spaTranslate("Discord would not take these as reactions:")} {posted.unusable.join(" ")}.
            </>
          )}
        </p>
      )}
    </div>
  );
}
