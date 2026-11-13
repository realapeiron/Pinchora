import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { ACCENTS } from "../../../lib/rules";
import type { EmbedField, EmbedSpec } from "../../../lib/api";
import { LIMITS, blankEmbed, embedChars } from "../../../lib/tags";
import { Field } from "../../../components/Field";
import { PixelButton } from "../../../components/PixelButton";

const text = (value: string | null) => value ?? "";

/** The embed of a reply: title, description, colour, author, footer, two pictures and up to 25 fields. */
export function EmbedEditor({
  embed,
  onChange,
  error,
}: {
  embed: EmbedSpec | null;
  onChange: (next: EmbedSpec | null) => void;
  error?: string | null;
}) {
  if (!embed) {
    return (
      <div className="embed-off">
        <PixelButton variant="secondary" onClick={() => onChange(blankEmbed())}>
          {" "}
          {deferredText("Add an embed")}{" "}
        </PixelButton>
        <p className="muted">
          {deferredText("A card under the text, with a coloured edge, a title, fields and pictures.")}
        </p>
      </div>
    );
  }
  const set = <K extends keyof EmbedSpec>(key: K, value: EmbedSpec[K]) =>
    onChange({ ...embed, [key]: value });
  const total = embedChars(embed);
  const editField = (index: number, patch: Partial<EmbedField>) =>
    set(
      "fields",
      embed.fields.map((f, i) => (i === index ? { ...f, ...patch } : f)),
    );
  return (
    <div className="embed-editor">
      <div className="embed-editor-head">
        <span className={`counter ${total > LIMITS.embed_total_max ? "is-over" : ""}`}>
          {total}/{LIMITS.embed_total_max} {spaTranslate("characters")}{" "}
        </span>
        <PixelButton variant="ghost" className="button-small" onClick={() => onChange(null)}>
          {" "}
          {deferredText("Remove the embed")}{" "}
        </PixelButton>
      </div>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <div className="form-grid form-grid-2">
        <Field label={spaTranslate("Title")} htmlFor="embed-title">
          <input
            id="embed-title"
            className="pixel-input"
            value={text(embed.title)}
            maxLength={LIMITS.embed_title_max}
            onChange={(event) => set("title", event.target.value)}
          />
        </Field>
        <Field
          label={spaTranslate("Colour")}
          htmlFor="embed-color"
          hint={deferredText("The edge of the card.")}
        >
          <div className="colour-row">
            <span
              className="colour-swatch"
              style={{
                background: /^#[0-9a-f]{6}$/i.test(text(embed.color)) ? text(embed.color) : "#ff6b4a",
              }}
              aria-hidden="true"
            />
            <input
              id="embed-color"
              className="pixel-input"
              value={text(embed.color)}
              placeholder={spaTranslate("#ff6b4a")}
              maxLength={7}
              onChange={(event) => set("color", event.target.value)}
            />
          </div>
          <div className="palette" role="group" aria-label={deferredText("Colour presets")}>
            {ACCENTS.map((accent) => (
              <button
                key={accent.hex}
                type="button"
                className={`palette-swatch ${text(embed.color).toLowerCase() === accent.hex ? "is-checked" : ""}`}
                style={{ background: accent.hex }}
                title={accent.name}
                aria-label={accent.name}
                onClick={() => set("color", accent.hex)}
              />
            ))}
          </div>
        </Field>
      </div>
      <Field label={spaTranslate("Description")} htmlFor="embed-description">
        <textarea
          id="embed-description"
          className="pixel-input pixel-textarea"
          rows={3}
          value={text(embed.description)}
          maxLength={LIMITS.embed_description_max}
          onChange={(event) => set("description", event.target.value)}
        />
      </Field>
      <div className="form-grid form-grid-2">
        <Field label={deferredText("Author line")} htmlFor="embed-author">
          <input
            id="embed-author"
            className="pixel-input"
            value={text(embed.author)}
            maxLength={LIMITS.embed_author_max}
            onChange={(event) => set("author", event.target.value)}
          />
        </Field>
        <Field label={deferredText("Footer")} htmlFor="embed-footer">
          <input
            id="embed-footer"
            className="pixel-input"
            value={text(embed.footer)}
            maxLength={LIMITS.embed_footer_max}
            onChange={(event) => set("footer", event.target.value)}
          />
        </Field>
        <Field
          label={deferredText("Picture address")}
          htmlFor="embed-image"
          hint={deferredText("A web address starting with https://.")}
        >
          <input
            id="embed-image"
            className="pixel-input"
            value={text(embed.image)}
            inputMode="url"
            onChange={(event) => set("image", event.target.value)}
          />
        </Field>
        <Field label={deferredText("Small picture address")} htmlFor="embed-thumbnail">
          <input
            id="embed-thumbnail"
            className="pixel-input"
            value={text(embed.thumbnail)}
            inputMode="url"
            onChange={(event) => set("thumbnail", event.target.value)}
          />
        </Field>
        <Field
          label={deferredText("Title link")}
          htmlFor="embed-url"
          hint={deferredText("Makes the title a link. Needs a title.")}
        >
          <input
            id="embed-url"
            className="pixel-input"
            value={text(embed.url)}
            inputMode="url"
            onChange={(event) => set("url", event.target.value)}
          />
        </Field>
      </div>
      <div className="embed-fields-editor">
        <span className="field-label">
          {" "}
          {deferredText("Fields (")}
          {embed.fields.length}/{LIMITS.embed_fields_max})
        </span>
        {embed.fields.map((field, index) => (
          <div key={index} className="embed-field-row">
            <input
              className="pixel-input"
              aria-label={deferredText("Field {arg1} name", { arg1: index + 1 })}
              placeholder={spaTranslate("Name")}
              value={field.name}
              maxLength={LIMITS.embed_field_name_max}
              onChange={(event) => editField(index, { name: event.target.value })}
            />
            <textarea
              className="pixel-input pixel-textarea"
              aria-label={deferredText("Field {arg1} value", { arg1: index + 1 })}
              placeholder={spaTranslate("Value")}
              rows={2}
              value={field.value}
              maxLength={LIMITS.embed_field_value_max}
              onChange={(event) => editField(index, { value: event.target.value })}
            />
            <label className="check">
              <input
                type="checkbox"
                checked={field.inline}
                onChange={(event) => editField(index, { inline: event.target.checked })}
              />{" "}
              {deferredText("Side by side")}{" "}
            </label>
            <PixelButton
              variant="ghost"
              className="button-small"
              aria-label={deferredText("Remove field {arg1}", { arg1: index + 1 })}
              onClick={() =>
                set(
                  "fields",
                  embed.fields.filter((_, i) => i !== index),
                )
              }
            >
              {spaTranslate("Remove")}
            </PixelButton>
          </div>
        ))}
        {embed.fields.length < LIMITS.embed_fields_max && (
          <div>
            <PixelButton
              variant="secondary"
              className="button-small"
              onClick={() => set("fields", [...embed.fields, { name: "", value: "", inline: false }])}
            >
              {" "}
              {deferredText("Add a field")}{" "}
            </PixelButton>
          </div>
        )}
      </div>
    </div>
  );
}
