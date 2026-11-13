import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import { roleColor } from "../../../components/Pickers";
import type { GuildInfo } from "../../../lib/api";
import { type PanelDraft, customEmojiUrl, footerOf, spanText } from "../api/roles";

/** One emoji: the character, or the custom emoji's picture. */
function Emoji({ text }: { text: string | null }) {
  const url = customEmojiUrl(text);
  if (url) return <img src={url} alt="" />;
  return <>{text}</>;
}

/** The panel as members will see it: the bot's message with its buttons, menu or reactions. */
export function PanelPreview({ draft, info }: { draft: PanelDraft; info: GuildInfo | undefined }) {
  const name = (id: string) => {
    const role = info?.roles.find((r) => r.id === id);
    return role ? `@${role.name}` : "@role";
  };
  const color = (id: string) => roleColor(info?.roles.find((r) => r.id === id));
  const rows = draft.roles.filter((r) => r.role_id);
  return (
    <div className="panel-preview" role="region" aria-label={deferredText("Panel preview")}>
      <div className="discord">
        <div className="discord-head">
          <span className="discord-avatar" aria-hidden="true" />
          <strong>Pinchora</strong>
          <span className="discord-tag">{spaTranslate("APP")}</span>
        </div>
        <div className="embed-card" style={{ borderLeftColor: "#ff6b4a" }}>
          <div className="embed-main">
            <p className="embed-title">{draft.title.trim() || "Untitled panel"}</p>
            <div className="embed-description">
              {draft.description.trim() && <span>{draft.description.trim()}</span>}
              {rows.length === 0 && <span className="muted">{deferredText("No roles yet.")}</span>}
              {rows.map((r) => (
                <span key={r.key} className="preview-line">
                  {r.emoji && (
                    <span className="preview-emoji">
                      <Emoji text={r.emoji} />
                    </span>
                  )}
                  <strong>{r.label || "Label"}</strong>·
                  <span className="preview-mention" style={{ color: color(r.role_id) }}>
                    {name(r.role_id)}
                  </span>
                  {r.description && draft.style !== "select" && (
                    <span className="muted">— {r.description}</span>
                  )}
                  {r.temp_secs ? <span className="muted">· ⏳ {spanText(r.temp_secs)}</span> : null}
                </span>
              ))}
              {draft.required_role && (
                <span className="muted">
                  {" "}
                  {spaTranslate("Needs")} <span className="preview-mention">{name(draft.required_role)}</span>
                </span>
              )}
            </div>
            <p className="embed-footer muted">{footerOf(draft)}</p>
          </div>
        </div>
        {draft.style === "buttons" && rows.length > 0 && (
          <div className="preview-buttons" role="group" aria-label={spaTranslate("Buttons")}>
            {rows.map((r) => (
              <span key={r.key} className="preview-button">
                {r.emoji && <Emoji text={r.emoji} />}
                {r.label || "Label"}
              </span>
            ))}
          </div>
        )}
        {draft.style === "select" && rows.length > 0 && (
          <>
            <div className="preview-select" role="group" aria-label={spaTranslate("Menu")}>
              <span>{spaTranslate("Pick your roles")}</span>
              <span aria-hidden="true">▾</span>
            </div>
            <div className="preview-select-menu">
              {rows.slice(0, 4).map((r) => (
                <div key={r.key}>
                  <span>
                    {r.emoji && (
                      <>
                        <Emoji text={r.emoji} />{" "}
                      </>
                    )}
                    {r.label || "Label"}
                  </span>
                  {r.description && <small>{r.description}</small>}
                </div>
              ))}
              {rows.length > 4 && (
                <small>
                  {spaTranslate("and")} {rows.length - 4} {deferredText("more…")}
                </small>
              )}
            </div>
          </>
        )}
        {draft.style === "reactions" && rows.length > 0 && (
          <div className="preview-reactions" role="group" aria-label={spaTranslate("Reactions")}>
            {rows.map((r) => (
              <span key={r.key} className="preview-reaction">
                {r.emoji ? <Emoji text={r.emoji} /> : "?"} 1
              </span>
            ))}
          </div>
        )}
      </div>
      <p className="preview-note muted">
        {" "}
        {deferredText(
          "Drawn here as Discord would show it. Nothing is posted until you press Post panel.",
        )}{" "}
      </p>
    </div>
  );
}
