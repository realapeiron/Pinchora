import { useOutletContext } from "react-router";
import { type Overview } from "../../../lib/api";
import { SettingsPage } from "../../../components/PageHead";
import { t as deferredText } from "../../../i18n/dashboard";
import { useMessageStudio } from "../hooks/useMessageStudio";
import { SavedKits } from "./studio/SavedKits";
import { StarterTemplates } from "./studio/StarterTemplates";
import { MessageFields } from "./studio/MessageFields";
import { MessageVariants } from "./studio/MessageVariants";
import { MessageEmbeds } from "./studio/MessageEmbeds";
import { MessageComponents } from "./studio/MessageComponents";
import { LivePreview } from "./studio/LivePreview";
import { DiscordPosting } from "./studio/DiscordPosting";
import { EventBindings } from "./studio/EventBindings";
import { SavedVersions } from "./studio/SavedVersions";
import { StudioSave } from "./studio/StudioSave";
import "../../../styles/message-studio.css";

export function MessageStudio() {
  const { guild } = useOutletContext<Overview>();
  return <StudioEditor key={guild.id} guild={guild} />;
}

function StudioEditor({ guild }: { guild: Overview["guild"] }) {
  const model = useMessageStudio(guild);
  return (
    <SettingsPage
      className="settings-page message-studio"
      title={deferredText("MESSAGE STUDIO")}
      lead={deferredText("Build, preview, save, and post Discord messages. Saved kits keep 20 versions.")}
    >
      <SavedKits
        kits={model.kits}
        kitKey={model.key}
        choose={model.choose}
        fresh={model.fresh}
        importJson={model.importJson}
        exportJson={model.exportJson}
      />
      <StarterTemplates useStarter={model.useStarter} />
      <MessageFields draft={model.draft} set={model.set} kitKey={model.key} />
      <MessageVariants draft={model.draft} set={model.set} />
      <MessageEmbeds draft={model.draft} set={model.set} />
      <MessageComponents draft={model.draft} set={model.set} editComponent={model.editComponent} />
      <LivePreview preview={model.preview} draft={model.draft} setVariant={model.setVariant} />
      <DiscordPosting
        discord={model.discord}
        channel={model.channel}
        setChannel={model.setChannel}
        messageId={model.messageId}
        setMessageId={model.setMessageId}
        kitKey={model.key}
        dirty={model.dirty}
        busy={model.busy}
        preview={model.preview}
        send={model.send}
      />
      <EventBindings bindings={model.bindings} kits={model.kits} bind={model.bind} />
      <SavedVersions revisions={model.revisions} busy={model.busy} restore={model.restore} />
      <StudioSave
        dirty={model.dirty}
        busy={model.busy}
        save={model.save}
        error={model.error}
        notice={model.notice}
      />
    </SettingsPage>
  );
}
