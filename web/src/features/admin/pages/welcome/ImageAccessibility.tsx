import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { Field } from "../../../../components/Field";
import { type WelcomeModel } from "../../hooks/useWelcomeEditor";

export function ImageAccessibility({ design, setDesign }: Pick<WelcomeModel, "design" | "setDesign">) {
  return (
    <Panel title={deferredText("IMAGE ACCESSIBILITY")}>
      <Field
        label={spaTranslate("Image alt text")}
        htmlFor="card-alt-text"
        hint={deferredText(
          "Describe the picture for screen readers. Discord receives this description with the attachment.",
        )}
      >
        <textarea
          id="card-alt-text"
          className="pixel-input"
          maxLength={1024}
          value={design.alt_text ?? ""}
          onChange={(event) => setDesign({ ...design, alt_text: event.target.value }, "alt-text")}
        />
        <span className="counter">{(design.alt_text ?? "").length}/1024</span>
      </Field>
    </Panel>
  );
}
