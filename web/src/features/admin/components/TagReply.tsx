import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { SwitchRow } from "../../../components/Field";
import { Panel } from "../../../components/Panel";
import { PixelToggle } from "../../../components/PixelToggle";
import type { CommandDef, ResponderDef, TriggerKind } from "../../../lib/api";
import { placeholdersFor } from "../../../lib/tags";
import type { TagEditor } from "../hooks/useTagEditor";
import { ResponsesEditor } from "./ResponsesEditor";
import { EmbedEditor } from "./EmbedEditor";

type ReplyDef = CommandDef | ResponderDef;

type Props<Def extends ReplyDef> = {
  draft: Pick<Def, "responses" | "embed" | "allow_mentions">;
  set: TagEditor<Def>["set"];
  problem: TagEditor<Def>["problem"];
  hasEmbed: boolean;
  placeholderKind: "command" | TriggerKind;
  mentionDescription: string;
};

export function TagReply<Def extends ReplyDef>({
  draft,
  set,
  problem,
  hasEmbed,
  placeholderKind,
  mentionDescription,
}: Props<Def>) {
  return (
    <Panel title={spaTranslate("REPLY")}>
      <ResponsesEditor
        responses={draft.responses}
        onChange={(next) => set("responses", next)}
        placeholders={placeholdersFor(placeholderKind)}
        error={problem("responses")}
        optional={hasEmbed}
      />
      <EmbedEditor embed={draft.embed} onChange={(next) => set("embed", next)} error={problem("embed")} />
      <SwitchRow
        title={deferredText("Allow mentions")}
        control={
          <PixelToggle
            label={deferredText("Allow mentions")}
            checked={draft.allow_mentions}
            onChange={(next) => set("allow_mentions", next)}
          />
        }
      >
        {" "}
        {mentionDescription}{" "}
      </SwitchRow>
    </Panel>
  );
}
