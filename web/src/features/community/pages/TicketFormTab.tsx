import { translate as spaTranslate } from "../../../i18n";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";
import type { TabProps } from "./TicketPanelTabProps";

export function FormTab({ draft, set, limits, errors, updateQuestion }: TabProps) {
  return (
    <Field
      label={spaTranslate("Questions")}
      error={errors.questions}
      hint={spaTranslate(
        "Asked in a form before the ticket opens; the answers are the ticket's first message. No questions, no form.",
      )}
      aside={
        <span className="counter">
          {draft.questions.length}/{limits.questions}
        </span>
      }
    >
      {draft.questions.length === 0 ? (
        <p className="empty-row muted">{spaTranslate("No form: the ticket opens straight away.")}</p>
      ) : (
        <ol className="tk-questions">
          {draft.questions.map((q, n) => (
            <li key={n} className="tk-question">
              <input
                className="pixel-input"
                aria-label={spaTranslate("Question {arg1}", { arg1: n + 1 })}
                placeholder={spaTranslate("What is your order number?")}
                value={q.label}
                maxLength={limits.question}
                onChange={(e) => updateQuestion(n, { label: e.target.value })}
              />
              <input
                className="pixel-input"
                aria-label={spaTranslate("Question {arg1} example", { arg1: n + 1 })}
                placeholder={spaTranslate("An example answer (optional)")}
                value={q.placeholder}
                maxLength={limits.placeholder}
                onChange={(e) => updateQuestion(n, { placeholder: e.target.value })}
              />
              <button
                type="button"
                className="tk-icon-button"
                aria-label={spaTranslate("Remove question {arg1}", { arg1: n + 1 })}
                onClick={() =>
                  set(
                    "questions",
                    draft.questions.filter((_, i) => i !== n),
                  )
                }
              >
                ×
              </button>
              <div className="tk-question-switches">
                <label className="tk-check">
                  <input
                    type="checkbox"
                    checked={q.long}
                    onChange={(e) => updateQuestion(n, { long: e.target.checked })}
                  />{" "}
                  {spaTranslate("Long answer")}{" "}
                </label>
                <label className="tk-check">
                  <input
                    type="checkbox"
                    checked={q.required}
                    onChange={(e) => updateQuestion(n, { required: e.target.checked })}
                  />{" "}
                  {spaTranslate("Required")}{" "}
                </label>
                <label className="tk-check">
                  <input
                    type="checkbox"
                    checked={q.upload ?? false}
                    onChange={(e) =>
                      updateQuestion(n, { upload: e.target.checked, choices: [], long: false })
                    }
                  />{" "}
                  {spaTranslate("File upload")}{" "}
                </label>
              </div>
              {!q.upload && (
                <input
                  className="pixel-input tk-question-choices"
                  aria-label={spaTranslate("Choices for question {arg1}", { arg1: n + 1 })}
                  placeholder={spaTranslate("Choices separated by commas (optional)")}
                  value={(q.choices ?? []).join(", ")}
                  onChange={(e) =>
                    updateQuestion(n, {
                      choices: e.target.value
                        .split(",")
                        .map((s) => s.trim())
                        .filter(Boolean),
                    })
                  }
                />
              )}
            </li>
          ))}
        </ol>
      )}
      <PixelButton
        variant="secondary"
        className="button-small tk-add"
        disabled={draft.questions.length >= limits.questions}
        onClick={() =>
          set("questions", [...draft.questions, { label: "", placeholder: "", long: false, required: true }])
        }
      >
        {" "}
        {spaTranslate("Add question")}{" "}
      </PixelButton>
    </Field>
  );
}
