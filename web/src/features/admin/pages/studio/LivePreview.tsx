import { t as deferredText } from "../../../../i18n/dashboard";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { MessagePreview } from "../../../../components/MessagePreview";

export function LivePreview({
  preview,
  draft,
  setVariant,
}: Pick<StudioModel, "preview" | "draft" | "setVariant">) {
  return (
    <Panel title={deferredText("LIVE DISCORD PREVIEW")}>
      {preview ? (
        <>
          <MessagePreview
            preview={{
              text: preview.content,
              embed: null,
              embeds: preview.embeds,
              components: preview.components,
              ephemeral: false,
              notes: [],
            }}
          />
          <p className="muted">
            {deferredText("Allowed mentions: {arg1}", {
              arg1:
                preview.allowed_mentions === "none"
                  ? deferredText("No pings")
                  : deferredText("Everyone and roles"),
            })}
          </p>
          {draft.variants.length > 0 && (
            <div className="studio-actions">
              <PixelButton variant="ghost" onClick={() => setVariant((v) => v + 1)}>
                {deferredText("Preview another variant")}
              </PixelButton>
            </div>
          )}
        </>
      ) : (
        <p className="muted">{deferredText("Add content or an embed to preview.")}</p>
      )}
    </Panel>
  );
}
