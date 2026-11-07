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
import { PageHead } from "./PageHead";
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
    <div className="settings-page starboard-page">
      <PageHead
        title="STARBOARD"
        lead="Give the best posts a place of their own. Stars move a post onto a board; vote channels add reactions to every new message."
      />
      {module && !module.enabled && (
        <p className="notice">
          <strong>Starboard is off.</strong> Turn it on in <Link to={`/g/${guild}/modules`}>Modules</Link> to
          count reactions.
        </p>
      )}
      {query.isPending && <Loading label="Reading starboards" />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <>
          {!query.data.message_content && (
            <p className="notice" data-testid="content-lock">
              <span className="badge badge-gold">NEEDS MESSAGE CONTENT</span> Board posts show the jump link,
              author, and source link. To copy message text and attachments, enable Discord's privileged
              Message Content intent for Pinchora. Media-only voting needs this intent too.
            </p>
          )}
          <Panel title={`BOARDS · ${query.data.boards.length}/${maxBoards} used`}>
            <p className="panel-lead muted">
              Each board has its own emoji and threshold. A message can appear on more than one board.
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
                      onSuccess: () => setNotice("Board removed."),
                      onError: (error) => setNotice(error.message),
                    });
                  }}
                />
              ))}
              {query.data.boards.length < maxBoards && (
                <button className="star-add" type="button" onClick={() => setEditing(newBoard())}>
                  <span>＋</span>
                  <strong>New board</strong>
                  <small>Choose a channel and an emoji</small>
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
                    setNotice("Board saved.");
                  },
                })
              }
            />
          )}
          <Panel title={`VOTE CHANNELS · ${query.data.votes.length} used`}>
            <p className="panel-lead muted">
              Pinchora adds the up reaction first, then the down reaction. A thread can open under each post.
            </p>
            <div className="vote-list">
              {query.data.votes.map((item) => (
                <div className="vote-row" key={item.channel_id}>
                  <span className="vote-emoji">
                    {item.up_emoji} {item.down_emoji}
                  </span>
                  <strong>{channelName(discord.data, item.channel_id)}</strong>
                  <small>
                    {item.only_media ? "Media only" : "All posts"}
                    {item.create_thread ? " · Threads" : ""}
                  </small>
                  <button type="button" onClick={() => setVote({ ...item })}>
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      removeVote.mutate(item.channel_id, {
                        onSuccess: () => setNotice("Vote channel removed."),
                        onError: (error) => setNotice(error.message),
                      })
                    }
                  >
                    Remove
                  </button>
                </div>
              ))}
              {query.data.votes.length === 0 && <p className="empty-row muted">No vote channels yet.</p>}
            </div>
            <PixelButton variant="secondary" onClick={() => setVote(newVote())}>
              Add vote channel
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
                    setNotice("Vote channel saved.");
                  },
                })
              }
            />
          )}
          {notice && (
            <p className="notice" role="status">
              {notice}{" "}
              <button type="button" onClick={() => setNotice(null)}>
                Dismiss
              </button>
            </p>
          )}
        </>
      )}
    </div>
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
        <span>⭐ {board.threshold} · #source</span>
        <strong>A standout post appears here</strong>
        <small>Author · jump link{messageContent ? " · attachments" : ""}</small>
      </div>
      <div className="star-actions">
        <button type="button" onClick={onEdit}>
          Edit board
        </button>
        <button type="button" onClick={onRemove}>
          Remove
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
    <Panel title={board.id ? "EDIT BOARD" : "NEW BOARD"}>
      <div className="star-form-grid">
        <label>
          Name
          <input
            className="pixel-input"
            value={board.name}
            maxLength={40}
            onChange={(e) => change({ name: e.target.value })}
          />
        </label>
        <label>
          Board channel
          <ChannelPicker
            info={info}
            value={board.channel_id || null}
            onChange={(v) => change({ channel_id: v ?? "" })}
            disabled={Boolean(board.id)}
          />
        </label>
        <label>
          Star emoji
          <input
            className="pixel-input"
            value={board.emoji}
            maxLength={64}
            onChange={(e) => change({ emoji: e.target.value })}
          />
        </label>
        <label>
          Threshold · {board.threshold}/50
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
          Colour
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
          Only from channels{" "}
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
            Leave empty for all channels. Private sources remain excluded to protect their audience.
          </small>
        </label>
        <label>
          Except channels{" "}
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
            ["self_star", "Allow self stars"],
            ["allow_bots", "Include bot posts and stars"],
            ["require_image", "Require an image"],
            ["remove_below", "Remove below threshold"],
            ["enabled", "Board enabled"],
          ] as const
        ).map(([key, label]) => (
          <PixelToggle key={key} label={label} checked={board[key]} onChange={(v) => change({ [key]: v })} />
        ))}
      </div>
      <div className="star-weights">
        <strong>Role weights · {Object.keys(board.role_weights).length}/25 used</strong>
        <p className="muted">
          Members with a weighted role count for its highest weight. Everyone else counts once.
        </p>
        <div className="star-weight-inputs">
          <select
            className="pixel-input pixel-select"
            aria-label="Weighted role"
            value={role}
            onChange={(e) => setRole(e.target.value)}
          >
            <option value="">Choose a role</option>
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
            aria-label="Role weight"
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
            Add weight
          </button>
        </div>
        {Object.entries(board.role_weights).map(([id, amount]) => (
          <span className="star-weight" key={id}>
            @{info?.roles.find((r) => r.id === id)?.name ?? id} ×{amount}{" "}
            <button
              type="button"
              aria-label={`Remove weight for ${id}`}
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
            {board.emoji} {board.threshold} · #source
          </span>
          <strong>{board.name}</strong>
          <small>
            Author · jump link{messageContent ? " · attachments" : ""}
            {board.require_image ? " · image" : ""}
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
          {preview ? "Hide preview" : "Preview board"}
        </PixelButton>
        <PixelButton variant="secondary" onClick={onCancel}>
          Cancel
        </PixelButton>
        <PixelButton
          variant="secondary"
          onClick={() => onSave(board)}
          disabled={
            busy || !board.channel_id || board.threshold < 1 || board.threshold > 50 || !board.name.trim()
          }
        >
          Save board
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
    <Panel title="VOTE CHANNEL">
      <div className="star-form-grid">
        <label>
          Channel
          <ChannelPicker
            info={info}
            value={vote.channel_id || null}
            onChange={(v) => change({ channel_id: v ?? "" })}
          />
        </label>
        <label>
          Up emoji
          <input
            className="pixel-input"
            value={vote.up_emoji}
            onChange={(e) => change({ up_emoji: e.target.value })}
          />
        </label>
        <label>
          Down emoji
          <input
            className="pixel-input"
            value={vote.down_emoji}
            onChange={(e) => change({ down_emoji: e.target.value })}
          />
        </label>
      </div>
      {!messageContent && (
        <p className="notice">
          <span className="badge badge-gold">NEEDS MESSAGE CONTENT</span> Media-only voting needs the Message
          Content intent.
        </p>
      )}
      <div className="star-switches">
        <PixelToggle
          label="Media posts only"
          disabled={!messageContent && !vote.only_media}
          checked={vote.only_media}
          onChange={(v) => change({ only_media: v })}
        />
        <PixelToggle
          label="Create a discussion thread"
          checked={vote.create_thread}
          onChange={(v) => change({ create_thread: v })}
        />
        <PixelToggle label="Enabled" checked={vote.enabled} onChange={(v) => change({ enabled: v })} />
      </div>
      {error && (
        <p className="field-error" role="alert">
          {error}
        </p>
      )}
      <div className="star-form-actions">
        <PixelButton variant="secondary" onClick={onCancel}>
          Cancel
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
          Save vote channel
        </PixelButton>
      </div>
    </Panel>
  );
}
