import { translate as spaTranslate } from "../i18n";
import { useRef } from "react";
import { flushSync } from "react-dom";

import { type MessageKind, LIMITS, MESSAGES, checkMessage } from "../lib/rules";
import { useMessagePreview } from "../lib/queries";
import { useDebounced } from "../lib/useDebounced";
import { Field } from "./Field";
import { PixelButton } from "./PixelButton";
import { RichText } from "./RichText";

/**
 * A welcome, goodbye or level-up message: the text (empty means the bot's default), chips that
 * insert the placeholders at the cursor, a character count, and the message as the bot would send it.
 */
export function MessageField({
  guild,
  kind,
  label,
  value,
  onChange,
  error,
}: {
  guild: string;
  kind: MessageKind;
  label: string;
  value: string | null;
  onChange: (next: string | null) => void;
  error?: string | null;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const meta = MESSAGES[kind];
  const id = `${kind}-message`;
  const text = value ?? "";
  const length = [...text].length;
  const settled = useDebounced(value, 350);
  const valid = settled === null || checkMessage(kind, settled).ok;
  const preview = useMessagePreview(guild, kind, settled, valid);

  const insert = (placeholder: string) => {
    const el = area.current;
    const base = value ?? meta.default;
    const [start, end] =
      el && value !== null ? [el.selectionStart, el.selectionEnd] : [base.length, base.length];
    const next = base.slice(0, start) + placeholder + base.slice(end);
    // Commit before placing the caret; a deferred focus can steal typing from the next field.
    flushSync(() => onChange(next));
    el?.focus();
    el?.setSelectionRange(start + placeholder.length, start + placeholder.length);
  };

  return (
    <Field
      label={label}
      htmlFor={id}
      error={error}
      aside={
        <span className={`counter ${length > LIMITS.message_max ? "is-over" : ""}`}>
          {length}/{LIMITS.message_max}
        </span>
      }
    >
      <textarea
        id={id}
        ref={area}
        className="pixel-input pixel-textarea"
        rows={3}
        value={text}
        placeholder={meta.default}
        onChange={(event) => onChange(event.target.value === "" ? null : event.target.value)}
        aria-invalid={error ? true : undefined}
      />
      <div className="chips" role="group" aria-label={spaTranslate("Insert a placeholder")}>
        {meta.placeholders.map((p) => (
          <button key={p} type="button" className="chip" onClick={() => insert(p)}>
            {p}
          </button>
        ))}
        {value !== null && (
          <PixelButton variant="ghost" className="chip-reset" onClick={() => onChange(null)}>
            {spaTranslate("Use default")}
          </PixelButton>
        )}
      </div>
      <div className="message-preview" aria-live="polite">
        <span className="pixel-label muted">{spaTranslate("PREVIEW")}</span>
        <p>
          {preview.data !== undefined ? (
            <RichText text={preview.data} />
          ) : (
            <span className="muted">{valid ? "…" : spaTranslate("Fix the message to see it.")}</span>
          )}
        </p>
      </div>
    </Field>
  );
}
