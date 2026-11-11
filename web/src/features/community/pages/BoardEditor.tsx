import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { ChannelPicker } from "../../../components/Pickers";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { boardSummary } from "./starboardSummary";
import { BoardOptions } from "./BoardOptions";
import { PixelButton } from "../../../components/PixelButton";
import type { GuildInfo } from "../../../lib/api";
import type { Board } from "../api/starboard";
export function BoardEditor({
  draft,
  info,
  messageContent,
  busy,
  error,
  onCancel,
  onSave,
}: {
  draft: Board;
  info: GuildInfo | undefined;
  messageContent: boolean;
  busy: boolean;
  error?: string;
  onCancel: () => void;
  onSave: (board: Board) => void;
}) {
  const [board, setBoard] = useState(draft);
  const [preview, setPreview] = useState(false);
  const change = (next: Partial<Board>) => setBoard({ ...board, ...next });
  return (
    <div className="settings-detail-list starboard-editor">
      <SettingsRow
        title={board.id ? spaTranslate("EDIT BOARD") : spaTranslate("NEW BOARD")}
        description={dt("Set the board channel, emoji and threshold.")}
        summary={boardSummary(board, info).details}
        initialOpen
      >
        <div className="settings-detail-fields">
          <div className="star-form-grid">
            <label>
              {spaTranslate("Name")}
              <input
                className="pixel-input"
                value={board.name}
                maxLength={40}
                onChange={(e) => change({ name: e.target.value })}
              />
            </label>
            <label>
              {" "}
              {spaTranslate("Board channel")}{" "}
              <ChannelPicker
                info={info}
                value={board.channel_id || null}
                onChange={(v) => change({ channel_id: v ?? "" })}
                disabled={Boolean(board.id)}
              />
            </label>
            <label>
              {" "}
              {spaTranslate("Star emoji")}{" "}
              <input
                className="pixel-input"
                value={board.emoji}
                maxLength={64}
                onChange={(e) => change({ emoji: e.target.value })}
              />
            </label>
            <label>
              {" "}
              {spaTranslate("Threshold ·")} {board.threshold}/50
              <input
                className="pixel-input"
                type="number"
                min={1}
                max={50}
                value={board.threshold}
                onChange={(e) => change({ threshold: Number(e.target.value) })}
              />
            </label>
            <label>
              {spaTranslate("Colour")}
              <input
                className="pixel-input"
                type="color"
                value={board.color}
                onChange={(e) => change({ color: e.target.value })}
              />
            </label>
          </div>
        </div>
        <div className="settings-detail-preview">
          <PixelButton variant="secondary" onClick={() => setPreview((v) => !v)}>
            {preview ? spaTranslate("Hide preview") : spaTranslate("Preview board")}
          </PixelButton>
          {preview && (
            <div
              className="star-preview star-live-preview"
              style={{ "--star-accent": board.color } as React.CSSProperties}
            >
              <span>
                {board.emoji} {board.threshold} {spaTranslate("· #source")}{" "}
              </span>
              <strong>{board.name}</strong>
              <small>
                {" "}
                {spaTranslate("Author · jump link")}
                {messageContent ? spaTranslate(" · attachments") : ""}
                {board.require_image ? spaTranslate(" · image") : ""}
              </small>
            </div>
          )}
        </div>
      </SettingsRow>
      <BoardOptions board={board} info={info} change={change} />
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <div className="star-form-actions">
        <PixelButton variant="secondary" onClick={onCancel}>
          {spaTranslate("Cancel")}
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => onSave(board)}
          disabled={
            busy || !board.channel_id || board.threshold < 1 || board.threshold > 50 || !board.name.trim()
          }
        >
          {" "}
          {spaTranslate("Save board")}{" "}
        </PixelButton>
      </div>
    </div>
  );
}
