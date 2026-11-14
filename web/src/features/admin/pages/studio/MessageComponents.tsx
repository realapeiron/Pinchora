import { Field } from "../../../../components/Field";
import { t as deferredText } from "../../../../i18n/dashboard";
import { translate as t } from "../../../../i18n";
import { Panel } from "../../../../components/Panel";
import { PixelButton } from "../../../../components/PixelButton";
import { type StudioModel } from "../../hooks/useMessageStudio";
import { componentRows } from "../../api/messageStudio";

export function MessageComponents({
  draft,
  set,
  editComponent,
}: Pick<StudioModel, "draft" | "set" | "editComponent">) {
  return (
    <Panel
      title={deferredText("BUTTONS AND SELECTS")}
      aside={<span className="counter">{componentRows(draft.components).length}/5</span>}
    >
      {draft.components.map((component, index) => (
        <div className="studio-component" key={index}>
          <div className="studio-actions">
            <strong>{component.type === "button" ? t("Button") : deferredText("Select menu")}</strong>
            <PixelButton
              variant="ghost"
              onClick={() =>
                set(
                  "components",
                  draft.components.filter((_, i) => i !== index),
                )
              }
            >
              {t("Remove")}
            </PixelButton>
          </div>
          {component.type === "button" ? (
            <div className="form-grid form-grid-2">
              <Field label={t("Label")} htmlFor={`button-label-${index}`}>
                <input
                  id={`button-label-${index}`}
                  className="pixel-input"
                  maxLength={80}
                  value={component.label}
                  onChange={(event) => editComponent(index, { ...component, label: event.target.value })}
                />
              </Field>
              <Field
                label={deferredText("HTTPS URL or leave blank for a reply")}
                htmlFor={`button-url-${index}`}
              >
                <input
                  id={`button-url-${index}`}
                  className="pixel-input"
                  value={component.url ?? ""}
                  onChange={(event) =>
                    editComponent(index, {
                      ...component,
                      url: event.target.value || null,
                      response: event.target.value ? null : (component.response ?? t("Reply")),
                    })
                  }
                />
              </Field>
              {!component.url && (
                <Field label={deferredText("Private reply")} htmlFor={`button-reply-${index}`}>
                  <input
                    id={`button-reply-${index}`}
                    className="pixel-input"
                    maxLength={2000}
                    value={component.response ?? ""}
                    onChange={(event) => editComponent(index, { ...component, response: event.target.value })}
                  />
                </Field>
              )}
            </div>
          ) : (
            <div>
              <Field label={t("Placeholder")} htmlFor={`select-placeholder-${index}`}>
                <input
                  id={`select-placeholder-${index}`}
                  className="pixel-input"
                  maxLength={150}
                  value={component.placeholder}
                  onChange={(event) =>
                    editComponent(index, { ...component, placeholder: event.target.value })
                  }
                />
              </Field>
              {component.options.map((option, optionIndex) => (
                <div className="studio-row" key={optionIndex}>
                  <input
                    className="pixel-input"
                    aria-label={deferredText("Option label")}
                    maxLength={100}
                    value={option.label}
                    onChange={(event) =>
                      editComponent(index, {
                        ...component,
                        options: component.options.map((o, i) =>
                          i === optionIndex ? { ...o, label: event.target.value } : o,
                        ),
                      })
                    }
                  />
                  <input
                    className="pixel-input"
                    aria-label={deferredText("Option value")}
                    maxLength={100}
                    value={option.value}
                    onChange={(event) =>
                      editComponent(index, {
                        ...component,
                        options: component.options.map((o, i) =>
                          i === optionIndex ? { ...o, value: event.target.value } : o,
                        ),
                      })
                    }
                  />
                  <input
                    className="pixel-input"
                    aria-label={deferredText("Private reply")}
                    maxLength={2000}
                    value={option.response}
                    onChange={(event) =>
                      editComponent(index, {
                        ...component,
                        options: component.options.map((o, i) =>
                          i === optionIndex ? { ...o, response: event.target.value } : o,
                        ),
                      })
                    }
                  />
                  <PixelButton
                    variant="ghost"
                    onClick={() =>
                      editComponent(index, {
                        ...component,
                        options: component.options.filter((_, i) => i !== optionIndex),
                      })
                    }
                  >
                    {t("Remove")}
                  </PixelButton>
                </div>
              ))}
              <PixelButton
                variant="secondary"
                disabled={component.options.length >= 25}
                onClick={() =>
                  editComponent(index, {
                    ...component,
                    options: [
                      ...component.options,
                      {
                        label: t("Choice"),
                        value: `choice_${component.options.length + 1}`,
                        response: deferredText("Selected!"),
                      },
                    ],
                  })
                }
              >
                {deferredText("Add choice")}
              </PixelButton>
            </div>
          )}
        </div>
      ))}
      <div className="studio-actions">
        <PixelButton
          variant="secondary"
          disabled={componentRows([...draft.components, { type: "button" }]).length > 5}
          onClick={() =>
            set("components", [
              ...draft.components,
              { type: "button", label: t("Button"), url: null, response: t("Reply") },
            ])
          }
        >
          {deferredText("Add button")}
        </PixelButton>
        <PixelButton
          variant="secondary"
          disabled={componentRows([...draft.components, { type: "select" }]).length > 5}
          onClick={() =>
            set("components", [
              ...draft.components,
              {
                type: "select",
                placeholder: t("Choose"),
                options: [{ label: t("Choice"), value: "choice_1", response: deferredText("Selected!") }],
              },
            ])
          }
        >
          {deferredText("Add select")}
        </PixelButton>
      </div>
    </Panel>
  );
}
