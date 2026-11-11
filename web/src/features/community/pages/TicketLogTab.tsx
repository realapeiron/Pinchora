import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/Pickers";
import type { TabProps } from "./TicketPanelTabProps";

export function LogTab({ draft, set, info, errors, base }: TabProps) {
  return (
    <>
      <Field
        label={spaTranslate("Log channel")}
        htmlFor={`${base}-log`}
        error={errors.log_channel_id}
        hint={spaTranslate(
          "Opened, claimed, reopened and closed tickets of this panel, with the transcript, if not the server's log channel.",
        )}
      >
        <ChannelPicker
          id={`${base}-log`}
          info={info}
          value={draft.log_channel_id}
          onChange={(id) => set("log_channel_id", id)}
          none={spaTranslate("The server's log channel")}
          invalid={!!errors.log_channel_id}
        />
      </Field>
      <p className="notice" role="note">
        {" "}
        {spaTranslate("Transcripts by direct message and ratings are server settings, above in")}{" "}
        <strong>{spaTranslate("Settings")}</strong>.
      </p>
    </>
  );
}
