import { translate as spaTranslate } from "../i18n";
import { useRef, useState } from "react";

import { type Placeholder, LIMITS } from "../lib/tags";
import { Field } from "./Field";
import { PixelButton } from "./PixelButton";
import { PlaceholderChips } from "./PlaceholderChips";

/**
 * The replies of a command or responder: one to five texts, one picked at random each time. Chips put
 * a placeholder at the cursor of the text last clicked into.
 */
export function ResponsesEditor({
  responses,
  onChange,
  placeholders,
  error,
  optional,
}: {
  responses: string[];
  onChange: (next: string[]) => void;
  placeholders: Placeholder[];
  error?: string | null;
  /** With an embed a reply may be empty: the embed is the answer. */
  optional?: boolean;
}) {
  const areas = useRef<(HTMLTextAreaElement | null)[]>([]);
  const [last, setLast] = useState(0);
  const edit = (index: number, text: string) => onChange(responses.map((r, i) => (i === index ? text : r)));

  const insert = (token: string) => {
    const at = Math.min(last, responses.length - 1);
    const el = areas.current[at];
    const text = responses[at] ?? "";
    const [start, end] = el ? [el.selectionStart, el.selectionEnd] : [text.length, text.length];
    edit(at, text.slice(0, start) + token + text.slice(end));
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(start + token.length, start + token.length);
    });
  };

  return (
    <Field
      label={responses.length > 1 ? spaTranslate("Replies") : spaTranslate("Reply")}
      htmlFor="reply-0"
      error={error}
      hint={
        responses.length > 1
          ? spaTranslate("One of these is picked at random each time.")
          : optional
            ? spaTranslate("Optional text above the embed.")
            : spaTranslate("What the bot says. Add more replies to have it pick one at random.")
      }
    >
      <div className="variants">
        {responses.map((text, index) => {
          const length = [...text].length;
          return (
            <div key={index} className="variant">
              <div className="variant-head">
                <span className="pixel-label muted">
                  {responses.length > 1 ? spaTranslate("REPLY {arg1}", { arg1: index + 1 }) : "TEXT"}
                </span>
                <span className={`counter ${length > LIMITS.response_max ? "is-over" : ""}`}>
                  {length}/{LIMITS.response_max}
                </span>
                {responses.length > 1 && (
                  <PixelButton
                    variant="ghost"
                    className="button-small"
                    aria-label={spaTranslate("Remove reply {arg1}", { arg1: index + 1 })}
                    onClick={() => {
                      onChange(responses.filter((_, i) => i !== index));
                      setLast(0);
                    }}
                  >
                    {spaTranslate("Remove")}
                  </PixelButton>
                )}
              </div>
              <textarea
                id={`reply-${index}`}
                aria-label={spaTranslate("Reply {arg1}", { arg1: index + 1 })}
                ref={(el) => {
                  areas.current[index] = el;
                }}
                className="pixel-input pixel-textarea"
                rows={3}
                value={text}
                onFocus={() => setLast(index)}
                onChange={(event) => edit(index, event.target.value)}
                aria-invalid={length > LIMITS.response_max || undefined}
              />
            </div>
          );
        })}
      </div>
      <PlaceholderChips placeholders={placeholders} onPick={insert} />
      {responses.length < LIMITS.max_responses && (
        <div>
          <PixelButton
            variant="secondary"
            className="button-small"
            onClick={() => {
              onChange([...responses, ""]);
              setLast(responses.length);
            }}
          >
            {" "}
            {spaTranslate("Add another reply (")}
            {responses.length}/{LIMITS.max_responses})
          </PixelButton>
        </div>
      )}
    </Field>
  );
}
