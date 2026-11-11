import { translate as spaTranslate } from "../../../i18n";
import type { GuildInfo } from "../../../lib/api";
import type { Board } from "../api/starboard";
import { channelName } from "./starboardChannel";
export function BoardCard({
  board,
  info,
  messageContent,
  onEdit,
  onRemove,
}: {
  board: Board;
  info: GuildInfo | undefined;
  messageContent: boolean;
  onEdit: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="star-card" style={{ "--star-accent": board.color } as React.CSSProperties}>
      <div className="star-card-top">
        <span className="star-emoji">{board.emoji}</span>
        <span className="badge">{board.enabled ? "LIVE" : "PAUSED"}</span>
      </div>
      <h3>{board.name}</h3>
      <p className="muted">
        {channelName(info, board.channel_id)} · {board.threshold} {board.threshold === 1 ? "star" : "stars"}
      </p>
      <div className="star-preview">
        <span>
          ⭐ {board.threshold} {spaTranslate("· #source")}
        </span>
        <strong>{spaTranslate("A standout post appears here")}</strong>
        <small>
          {spaTranslate("Author · jump link")}
          {messageContent ? spaTranslate(" · attachments") : ""}
        </small>
      </div>
      <div className="star-actions">
        <button type="button" onClick={onEdit}>
          {" "}
          {spaTranslate("Edit board")}{" "}
        </button>
        <button type="button" onClick={onRemove}>
          {spaTranslate("Remove")}
        </button>
      </div>
    </div>
  );
}
