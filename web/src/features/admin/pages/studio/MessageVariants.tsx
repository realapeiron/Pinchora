import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";

export function MessageVariants({ draft, set }: Pick<StudioModel, "draft" | "set">) {
  return (
    <Panel
      title={deferredText("RANDOM MESSAGE GROUP")}
      aside={<span className="counter">{draft.variants.length}/10</span>}
    >
      <p className="muted">{deferredText("When variants exist, one is chosen at random for each post.")}</p>
      {draft.variants.map((text, index) => (
        <div className="studio-row" key={index}>
          <textarea
            className="pixel-input"
            aria-label={deferredText("Variant {arg1}", { arg1: index + 1 })}
            maxLength={2000}
            value={text}
            onChange={(event) =>
              set(
                "variants",
                draft.variants.map((v, i) => (i === index ? event.target.value : v)),
              )
            }
          />
          <PixelButton
            variant="ghost"
            onClick={() =>
              set(
                "variants",
                draft.variants.filter((_, i) => i !== index),
              )
            }
          >
            {t("Remove")}
          </PixelButton>
        </div>
      ))}
      <PixelButton
        variant="secondary"
        disabled={draft.variants.length >= 10}
        onClick={() => set("variants", [...draft.variants, draft.content || deferredText("Hello {user}!")])}
      >
        {deferredText("Add variant")}
      </PixelButton>
    </Panel>
  );
}
