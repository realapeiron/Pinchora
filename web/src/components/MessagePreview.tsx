import { translate as spaTranslate } from "../i18n";
import type { EmbedSpec, Preview } from "../lib/api";
import { RichText } from "./RichText";

/** The host of a picture address, for the stand-in box (the dashboard never loads pictures from other sites). */
function host(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

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
        {embed.image && <p className="embed-picture">picture from {host(embed.image)}</p>}
        {embed.footer && <p className="embed-footer muted">{embed.footer}</p>}
      </div>
      {embed.thumbnail && <p className="embed-thumb">small picture from {host(embed.thumbnail)}</p>}
    </div>
  );
}

/** A message the way Discord shows it: the bot, the text, the embed; with the notes the server added. */
export function MessagePreview({ preview, label = "Message preview" }: { preview: Preview; label?: string }) {
  return (
    <div className="discord" role="region" aria-label={label}>
      <div className="discord-head">
        <span className="discord-avatar" aria-hidden="true" />
        <strong>Pinchora</strong>
        <span className="discord-tag">APP</span>
        {preview.ephemeral && (
          <span className="muted discord-private">{spaTranslate("Only you can see this")}</span>
        )}
      </div>
      {preview.text && (
        <p className="discord-text">
          <RichText text={preview.text} />
        </p>
      )}
      {preview.embed && <EmbedCard embed={preview.embed} />}
      {!preview.text && !preview.embed && <p className="muted">The bot would say nothing.</p>}
    </div>
  );
}
