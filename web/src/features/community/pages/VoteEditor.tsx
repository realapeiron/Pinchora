import { useState } from "react";
import { translate as spaTranslate } from "../../../i18n";
import { ChannelPicker } from "../../../components/Pickers";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { voteSummary } from "./starboardSummary";
import { PixelButton } from "../../../components/PixelButton";
import type { GuildInfo } from "../../../lib/api";
import type { VoteChannel } from "../api/starboard";
export function VoteEditor({
  draft,
  info,
  messageContent,
  busy,
  error,
  onCancel,
  onSave,
}: {
  draft: VoteChannel;
  messageContent: boolean;
  info: GuildInfo | undefined;
  busy: boolean;
  error?: string;
  onCancel: () => void;
  onSave: (vote: VoteChannel) => void;
}) {
  const [vote, setVote] = useState(draft);
  const change = (next: Partial<VoteChannel>) => setVote({ ...vote, ...next });
  return (
    <div className="settings-detail-list starboard-editor">
      <SettingsRow
        rowId="vote-editor-vote-channel"
        title={dt("Vote channel", {}, "VOTE CHANNEL")}
        description={dt("Set reactions and voting behavior for a channel.")}
        summary={voteSummary(vote, info)}
        initialOpen
      >
        <div className="settings-detail-fields">
          <div className="star-form-grid">
            <label>
              {spaTranslate("Channel")}
              <ChannelPicker
                info={info}
                value={vote.channel_id || null}
                onChange={(v) => change({ channel_id: v ?? "" })}
              />
            </label>
            <label>
              {" "}
              {spaTranslate("Up emoji")}{" "}
              <input
                className="pixel-input"
                value={vote.up_emoji}
                onChange={(e) => change({ up_emoji: e.target.value })}
              />
            </label>
            <label>
              {" "}
              {spaTranslate("Down emoji")}{" "}
              <input
                className="pixel-input"
                value={vote.down_emoji}
                onChange={(e) => change({ down_emoji: e.target.value })}
              />
            </label>
          </div>
          {!messageContent && (
            <p className="notice">
              <span className="badge badge-gold">{spaTranslate("NEEDS MESSAGE CONTENT")}</span>{" "}
              {spaTranslate("Media-only voting needs the Message Content intent.")}{" "}
            </p>
          )}
        </div>
      </SettingsRow>
      <SettingsRow
        rowId="vote-editor-media-posts-only"
        title={spaTranslate("Media posts only")}
        description={dt("Limit voting to posts with media.")}
        summary={dt(vote.only_media ? "On" : "Off")}
        enabled={vote.only_media}
        onToggle={(v) => change({ only_media: v })}
        toggleDisabled={!messageContent && !vote.only_media}
      >
        <p className="muted">{dt("Limit voting to posts with media.")}</p>
      </SettingsRow>
      <SettingsRow
        rowId="vote-editor-create-a-discussion-thread"
        title={spaTranslate("Create a discussion thread")}
        description={dt("Open a thread under each post.")}
        summary={dt(vote.create_thread ? "On" : "Off")}
        enabled={vote.create_thread}
        onToggle={(v) => change({ create_thread: v })}
      >
        <p className="muted">{dt("Open a thread under each post.")}</p>
      </SettingsRow>
      <SettingsRow
        rowId="vote-editor-enabled"
        title={spaTranslate("Enabled")}
        description={dt("Add voting reactions in this channel.")}
        summary={dt(vote.enabled ? "On" : "Off")}
        enabled={vote.enabled}
        onToggle={(v) => change({ enabled: v })}
      >
        <p className="muted">{dt("Add voting reactions in this channel.")}</p>
      </SettingsRow>
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
          onClick={() => onSave(vote)}
          disabled={
            busy ||
            !vote.channel_id ||
            !vote.up_emoji ||
            !vote.down_emoji ||
            vote.up_emoji === vote.down_emoji
          }
        >
          {" "}
          {spaTranslate("Save vote channel")}{" "}
        </PixelButton>
      </div>
    </div>
  );
}
