import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { ChannelPicker } from "../../../components/DashboardPickers";
import type { TabProps } from "./TicketPanelTabProps";

export function LogTab({ draft, set, info, errors, base }: TabProps) {
  return (
    <>
      <Field
        label={deferredText("Log channel")}
        htmlFor={`${base}-log`}
        error={errors.log_channel_id}
        hint={deferredText(
          "Opened, claimed, reopened and closed tickets of this panel, with the transcript, if not the server's log channel.",
        )}
      >
        <ChannelPicker
          posting
          id={`${base}-log`}
          info={info}
          value={draft.log_channel_id}
          onChange={(id) => set("log_channel_id", id)}
          none={deferredText("The server's log channel")}
          invalid={!!errors.log_channel_id}
        />
      </Field>
      <p className="notice" role="note">
        {" "}
        {deferredText("Transcripts by direct message and ratings are server settings, above in")}{" "}
        <strong>{spaTranslate("Settings")}</strong>.
      </p>
    </>
  );
}
