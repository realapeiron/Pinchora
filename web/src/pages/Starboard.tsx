import { translate as spaTranslate } from "../i18n";
import "../styles/starboard.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";

import { Panel } from "../components/Panel";
import { ChannelPicker } from "../components/Pickers";
import { PixelButton } from "../components/PixelButton";
import { PixelToggle } from "../components/PixelToggle";
import { type GuildInfo, type Overview } from "../lib/api";
import { useDiscord } from "../lib/queries";
import { type Board, type VoteChannel, newBoard, newVote, useStarboard } from "../lib/starboard";
import { SettingsPage } from "./PageHead";
import { Loading, Problem } from "./States";

const maxBoards = 3;

export function Starboard() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const { query, saveBoard, removeBoard, saveVote, removeVote } = useStarboard(guild);
  const discord = useDiscord(guild);
  const [editing, setEditing] = useState<Board | null>(null);
  const [vote, setVote] = useState<VoteChannel | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const module = data.modules.find((m) => m.key === "starboard");
  return (
    <SettingsPage
      className="settings-page starboard-page"
      title={spaTranslate("STARBOARD")}
      lead={spaTranslate(
        "Give the best posts a place of their own. Stars move a post onto a board; vote channels add reactions to every new message.",
      )}
    >
      {module && !module.enabled && (
        <p className="notice">
          <strong>{spaTranslate("Starboard is off.")}</strong> {spaTranslate("Turn it on in")}{" "}
          <Link to={`/g/${guild}/modules`}>{spaTranslate("Modules")}</Link>{" "}
          {spaTranslate("to count reactions.")}{" "}
        </p>
      )}
      {query.isPending && <Loading label={spaTranslate("Reading starboards")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <>
          {!query.data.message_content && (
            <p className="notice" data-testid="content-lock">
              <span className="badge badge-gold">{spaTranslate("NEEDS MESSAGE CONTENT")}</span>{" "}
              {spaTranslate(
                "Board posts show the jump link, author, and source link. To copy message text and attachments, enable Discord's privileged Message Content intent for Pinchora. Media-only voting needs this intent too.",
              )}{" "}
            </p>
          )}
          <Panel
            title={spaTranslate("BOARDS · {arg1}/{arg2} used", {
              arg1: query.data.boards.length,
              arg2: maxBoards,
            })}
          >
            <p className="panel-lead muted">
              {" "}
              {spaTranslate(
                "Each board has its own emoji and threshold. A message can appear on more than one board.",
              )}{" "}
            </p>
            <div className="star-grid">
              {query.data.boards.map((board) => (
                <BoardCard
                  key={board.id}
                  board={board}
                  info={discord.data}
                  messageContent={query.data.message_content}
                  onEdit={() => setEditing({ ...board })}
                  onRemove={() => {
                    if (!board.id) return;
                    removeBoard.mutate(board.id, {
                      onSuccess: () => setNotice(spaTranslate("Board removed.")),
                      onError: (error) => setNotice(error.message),
                    });
                  }}
                />
              ))}
              {query.data.boards.length < maxBoards && (
                <button className="star-add" type="button" onClick={() => setEditing(newBoard())}>
                  <span>＋</span>
                  <strong>{spaTranslate("New board")}</strong>
                  <small>{spaTranslate("Choose a channel and an emoji")}</small>
                </button>
              )}
            </div>
          </Panel>
          {editing && (
            <BoardEditor
              key={editing.id ?? "new"}
              draft={editing}
              info={discord.data}
              messageContent={query.data.message_content}
              busy={saveBoard.isPending}
              error={saveBoard.error?.message}
              onCancel={() => setEditing(null)}
              onSave={(next) =>
                saveBoard.mutate(next, {
                  onSuccess: () => {
                    setEditing(null);
                    setNotice(spaTranslate("Board saved."));
                  },
                })
              }
            />
          )}
          <Panel title={spaTranslate("VOTE CHANNELS · {arg1} used", { arg1: query.data.votes.length })}>
            <p className="panel-lead muted">
              {" "}
              {spaTranslate(
                "Pinchora adds the up reaction first, then the down reaction. A thread can open under each post.",
              )}{" "}
            </p>
            <div className="vote-list">
              {query.data.votes.map((item) => (
                <div className="vote-row" key={item.channel_id}>
                  <span className="vote-emoji">
                    {item.up_emoji} {item.down_emoji}
                  </span>
                  <strong>{channelName(discord.data, item.channel_id)}</strong>
                  <small>
                    {item.only_media ? spaTranslate("Media only") : spaTranslate("All posts")}
                    {item.create_thread ? spaTranslate(" · Threads") : ""}
                  </small>
                  <button type="button" onClick={() => setVote({ ...item })}>
                    {spaTranslate("Edit")}
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      removeVote.mutate(item.channel_id, {
                        onSuccess: () => setNotice(spaTranslate("Vote channel removed.")),
                        onError: (error) => setNotice(error.message),
                      })
                    }
                  >
                    {spaTranslate("Remove")}
                  </button>
                </div>
              ))}
              {query.data.votes.length === 0 && (
                <p className="empty-row muted">{spaTranslate("No vote channels yet.")}</p>
              )}
            </div>
            <PixelButton variant="secondary" onClick={() => setVote(newVote())}>
              {" "}
              {spaTranslate("Add vote channel")}{" "}
            </PixelButton>
          </Panel>
          {vote && (
            <VoteEditor
              key={vote.channel_id || "new"}
              draft={vote}
              info={discord.data}
              messageContent={query.data.message_content}
              busy={saveVote.isPending}
              error={saveVote.error?.message}
              onCancel={() => setVote(null)}
              onSave={(next) =>
                saveVote.mutate(next, {
                  onSuccess: () => {
                    setVote(null);
                    setNotice(spaTranslate("Vote channel saved."));
                  },
                })
              }
            />
          )}
          {notice && (
            <p className="notice" role="status">
              {notice}{" "}
              <button type="button" onClick={() => setNotice(null)}>
                {spaTranslate("Dismiss")}
              </button>
            </p>
          )}
        </>
      )}
    </SettingsPage>
  );
}

function channelName(info: GuildInfo | undefined, id: string): string {
  return `#${info?.channels.find((c) => c.id === id)?.name ?? id}`;
}

function BoardCard({
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

function BoardEditor({
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
  const [role, setRole] = useState("");
  const [weight, setWeight] = useState(2);
  const change = (next: Partial<Board>) => setBoard({ ...board, ...next });
  const channels = info?.channels.filter((c) => c.type === 0 || c.type === 5) ?? [];
  return (
    <Panel title={board.id ? spaTranslate("EDIT BOARD") : spaTranslate("NEW BOARD")}>
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
      <div className="star-options">
        <label>
          {" "}
          {spaTranslate("Only from channels")}{" "}
          <select
            className="pixel-input pixel-select"
            multiple
            value={board.only_channels}
            onChange={(e) => change({ only_channels: Array.from(e.target.selectedOptions, (o) => o.value) })}
          >
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <small>
            {" "}
            {spaTranslate(
              "Leave empty for all channels. Private sources remain excluded to protect their audience.",
            )}{" "}
          </small>
        </label>
        <label>
          {" "}
          {spaTranslate("Except channels")}{" "}
          <select
            className="pixel-input pixel-select"
            multiple
            value={board.except_channels}
            onChange={(e) =>
              change({ except_channels: Array.from(e.target.selectedOptions, (o) => o.value) })
            }
          >
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="star-switches">
        {(
          [
            ["self_star", spaTranslate("Allow self stars")],
            ["allow_bots", spaTranslate("Include bot posts and stars")],
            ["require_image", spaTranslate("Require an image")],
            ["remove_below", spaTranslate("Remove below threshold")],
            ["enabled", spaTranslate("Board enabled")],
          ] as const
        ).map(([key, label]) => (
          <PixelToggle key={key} label={label} checked={board[key]} onChange={(v) => change({ [key]: v })} />
        ))}
      </div>
      <div className="star-weights">
        <strong>
          {spaTranslate("Role weights ·")} {Object.keys(board.role_weights).length}
          {spaTranslate("/25 used")}
        </strong>
        <p className="muted">
          {" "}
          {spaTranslate(
            "Members with a weighted role count for its highest weight. Everyone else counts once.",
          )}{" "}
        </p>
        <div className="star-weight-inputs">
          <select
            className="pixel-input pixel-select"
            aria-label={spaTranslate("Weighted role")}
            value={role}
            onChange={(e) => setRole(e.target.value)}
          >
            <option value="">{spaTranslate("Choose a role")}</option>
            {(info?.roles ?? [])
              .filter((r) => !r.everyone)
              .map((r) => (
                <option key={r.id} value={r.id}>
                  @{r.name}
                </option>
              ))}
          </select>
          <input
            className="pixel-input"
            aria-label={spaTranslate("Role weight")}
            type="number"
            min={1}
            max={50}
            value={weight}
            onChange={(e) => setWeight(Number(e.target.value))}
          />
          <button
            type="button"
            disabled={!role || weight < 1 || weight > 50 || Object.keys(board.role_weights).length >= 25}
            onClick={() => change({ role_weights: { ...board.role_weights, [role]: weight } })}
          >
            {" "}
            {spaTranslate("Add weight")}{" "}
          </button>
        </div>
        {Object.entries(board.role_weights).map(([id, amount]) => (
          <span className="star-weight" key={id}>
            @{info?.roles.find((r) => r.id === id)?.name ?? id} ×{amount}{" "}
            <button
              type="button"
              aria-label={spaTranslate("Remove weight for {arg1}", { arg1: id })}
              onClick={() => {
                const next = { ...board.role_weights };
                delete next[id];
                change({ role_weights: next });
              }}
            >
              ×
            </button>
          </span>
        ))}
      </div>
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
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <div className="star-form-actions">
        <PixelButton variant="secondary" onClick={() => setPreview((v) => !v)}>
          {preview ? spaTranslate("Hide preview") : spaTranslate("Preview board")}
        </PixelButton>
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
    </Panel>
  );
}

function VoteEditor({
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
    <Panel title={spaTranslate("VOTE CHANNEL")}>
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
      <div className="star-switches">
        <PixelToggle
          label={spaTranslate("Media posts only")}
          disabled={!messageContent && !vote.only_media}
          checked={vote.only_media}
          onChange={(v) => change({ only_media: v })}
        />
        <PixelToggle
          label={spaTranslate("Create a discussion thread")}
          checked={vote.create_thread}
          onChange={(v) => change({ create_thread: v })}
        />
        <PixelToggle
          label={spaTranslate("Enabled")}
          checked={vote.enabled}
          onChange={(v) => change({ enabled: v })}
        />
      </div>
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
    </Panel>
  );
}
