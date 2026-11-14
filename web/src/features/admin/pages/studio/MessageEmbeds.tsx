import { t as deferredText } from "../../../../i18n/dashboard";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { EmbedEditor } from "../../components/EmbedEditor";
import { blankEmbed } from "../../../../lib/tags";

export function MessageEmbeds({ draft, set }: Pick<StudioModel, "draft" | "set">) {
  return (
    <Panel title={deferredText("EMBEDS")} aside={<span className="counter">{draft.embeds.length}/10</span>}>
      {draft.embeds.map((embed, index) => (
        <div className="studio-embed" key={index}>
          <h3>{deferredText("Embed {arg1}", { arg1: index + 1 })}</h3>
          <EmbedEditor
            embed={embed}
            onChange={(next) =>
              set(
                "embeds",
                next
                  ? draft.embeds.map((e, i) => (i === index ? next : e))
                  : draft.embeds.filter((_, i) => i !== index),
              )
            }
          />
        </div>
      ))}
      <PixelButton
        variant="secondary"
        disabled={draft.embeds.length >= 10}
        onClick={() => set("embeds", [...draft.embeds, blankEmbed()])}
      >
        {deferredText("Add an embed")}
      </PixelButton>
    </Panel>
  );
}
