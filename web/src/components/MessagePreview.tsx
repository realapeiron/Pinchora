import { translate as spaTranslate } from "../i18n";
import type { EmbedSpec, Preview } from "../lib/api";
import { componentRows } from "../features/admin/api/messageStudio";
import { RichText } from "./RichText";

function EmbedCard({ embed }: { embed: EmbedSpec }) {
  return (
    <div className="embed-card" style={{ borderLeftColor: embed.color ?? "#ff6b4a" }}>
      <div className="embed-main">
        {embed.author && <p className="embed-author">{embed.author}</p>}
        {embed.title && (
          <p className="embed-title">
            <RichText text={embed.title} />
            {embed.url && <span className="muted"> ↗</span>}
          </p>
        )}
        {embed.description && (
          <p className="embed-description">
            <RichText text={embed.description} />
          </p>
        )}
        {embed.fields.length > 0 && (
          <div className="embed-fields">
            {embed.fields.map((f, i) => (
              <div key={i} className={`embed-field ${f.inline ? "is-inline" : ""}`}>
                <p className="embed-field-name">
                  <RichText text={f.name} />
                </p>
                <p>
                  <RichText text={f.value} />
                </p>
              </div>
            ))}
          </div>
        )}
        {embed.image && <img className="embed-picture" src={embed.image} alt={spaTranslate("Embed image")} />}
        {embed.footer && <p className="embed-footer muted">{embed.footer}</p>}
      </div>
      {embed.thumbnail && (
        <img className="embed-thumb" src={embed.thumbnail} alt={spaTranslate("Embed thumbnail")} />
      )}
    </div>
  );
}

/** A message the way Discord shows it: the bot, the text, the embed; with the notes the server added. */
export function MessagePreview({
  preview,
  label = spaTranslate("Message preview"),
}: {
  preview: Preview & {
    embeds?: EmbedSpec[];
    components?: (
      | { type: "button"; label: string }
      | { type: "select"; placeholder: string; options: { label: string }[] }
    )[];
  };
  label?: string;
}) {
  return (
    <div className="discord" role="region" aria-label={label}>
      <div className="discord-head">
        <span className="discord-avatar" aria-hidden="true" />
        <strong>Pinchora</strong>
        <span className="discord-tag">{spaTranslate("APP")}</span>
        {preview.ephemeral && (
          <span className="muted discord-private">{spaTranslate("Only you can see this")}</span>
        )}
      </div>
      {preview.text && (
        <p className="discord-text">
          <RichText text={preview.text} />
        </p>
      )}
      {(preview.embeds ?? (preview.embed ? [preview.embed] : [])).map((embed, index) => (
        <EmbedCard key={index} embed={embed} />
      ))}
      {componentRows(preview.components ?? []).map((row, index) => (
        <div className="studio-component-preview" key={index}>
          {row.map((component, child) =>
            component.type === "button" ? (
              <button key={child} type="button" disabled>
                {component.label}
              </button>
            ) : (
              <select key={child} disabled aria-label={component.placeholder}>
                <option>{component.placeholder}</option>
                {component.options.map((option, i) => (
                  <option key={i}>{option.label}</option>
                ))}
              </select>
            ),
          )}
        </div>
      ))}
      {!preview.text && !preview.embed && !preview.embeds?.length && (
        <p className="muted">{spaTranslate("The bot would say nothing.")}</p>
      )}
    </div>
  );
}
