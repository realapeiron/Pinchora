import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { Field } from "../../../../components/Field";

export function EventBindings({ bindings, kits, bind }: Pick<StudioModel, "bindings" | "kits" | "bind">) {
  return (
    <Panel title={deferredText("AUTOMATIC EVENT MESSAGES")}>
      <p className="muted">
        {deferredText(
          "Assign a text-only kit to welcome, goodbye, boost, or level up. Random variants and member conditions are evaluated when the event happens.",
        )}
      </p>
      <div className="form-grid form-grid-2">
        {(["welcome", "goodbye", "boost", "level_up"] as const).map((kind) => (
          <Field
            key={kind}
            label={t(
              kind === "level_up"
                ? "Level up"
                : kind === "boost"
                  ? "Boost"
                  : kind === "goodbye"
                    ? "Goodbye"
                    : "Welcome",
            )}
            htmlFor={`event-${kind}`}
          >
            <select
              id={`event-${kind}`}
              className="pixel-input pixel-select"
              value={bindings[kind] ?? ""}
              onChange={(event) => void bind(kind, event.target.value)}
            >
              <option value="">{deferredText("Default message")}</option>
              {kits
                .filter((item) => item.payload.embeds.length === 0 && item.payload.components.length === 0)
                .map((item) => (
                  <option key={item.key} value={item.key}>
                    {item.name}
                  </option>
                ))}
            </select>
          </Field>
        ))}
      </div>
    </Panel>
  );
}
