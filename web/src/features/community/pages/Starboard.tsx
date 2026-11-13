import { t as deferredText } from "../../../i18n/dashboard";
import { translate as spaTranslate } from "../../../i18n";
import "../../../styles/starboard.css";

import { useState } from "react";
import { Link, useOutletContext } from "react-router";

import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { starboardSummary } from "./starboardSummary";
import { BoardCard } from "./BoardCard";
import { BoardEditor } from "./BoardEditor";
import { VoteEditor } from "./VoteEditor";
import { channelName } from "./starboardChannel";
import { PixelButton } from "../../../components/PixelButton";
import { type Overview } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { type Board, type VoteChannel, newBoard, newVote, useStarboard } from "../api/starboard";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";

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
      title={deferredText("STARBOARD")}
      lead={deferredText(
        "Give the best posts a place of their own. Stars move a post onto a board; vote channels add reactions to every new message.",
      )}
    >
      {module && !module.enabled && (
        <p className="notice">
          <strong>{deferredText("Starboard is off.")}</strong> {deferredText("Turn it on in")}{" "}
          <Link to={`/g/${guild}/modules`}>{spaTranslate("Modules")}</Link>{" "}
          {deferredText("to count reactions.")}{" "}
        </p>
      )}
      {query.isPending && <Loading label={deferredText("Reading starboards")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && (
        <div className="settings-detail-list">
          {!query.data.message_content && (
            <p className="notice" data-testid="content-lock">
              <span className="badge badge-gold">{deferredText("NEEDS MESSAGE CONTENT")}</span>{" "}
              {deferredText(
                "Board posts show the jump link, author, and source link. To copy message text and attachments, enable Discord's privileged Message Content intent for Pinchora. Media-only voting needs this intent too.",
              )}{" "}
            </p>
          )}
          <SettingsRow
            rowId="starboard-boards"
            title={dt(
              "Boards · {arg1}/{arg2} used",
              {
                arg1: query.data.boards.length,
                arg2: maxBoards,
              },
              "BOARDS · {arg1}/{arg2} used",
            )}
            description={dt("Manage the boards that collect standout posts.")}
            summary={starboardSummary(query.data).boards}
            initialOpen
          >
            <p className="panel-lead muted">
              {" "}
              {deferredText(
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
                      onSuccess: () => setNotice(deferredText("Board removed.")),
                      onError: (error) => setNotice(error.message),
                    });
                  }}
                />
              ))}
              {query.data.boards.length < maxBoards && (
                <button className="star-add" type="button" onClick={() => setEditing(newBoard())}>
                  <span>＋</span>
                  <strong>{spaTranslate("New board")}</strong>
                  <small>{deferredText("Choose a channel and an emoji")}</small>
                </button>
              )}
            </div>
          </SettingsRow>
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
                    setNotice(deferredText("Board saved."));
                  },
                })
              }
            />
          )}
          <SettingsRow
            rowId="starboard-vote-channels"
            title={dt(
              "Vote channels · {arg1} used",
              { arg1: query.data.votes.length },
              "VOTE CHANNELS · {arg1} used",
            )}
            description={dt("Choose where every post receives voting reactions.")}
            summary={starboardSummary(query.data).votes}
          >
            <p className="panel-lead muted">
              {" "}
              {deferredText(
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
                    {item.only_media ? deferredText("Media only") : deferredText("All posts")}
                    {item.create_thread ? deferredText(" · Threads") : ""}
                  </small>
                  <button type="button" onClick={() => setVote({ ...item })}>
                    {spaTranslate("Edit")}
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      removeVote.mutate(item.channel_id, {
                        onSuccess: () => setNotice(deferredText("Vote channel removed.")),
                        onError: (error) => setNotice(error.message),
                      })
                    }
                  >
                    {spaTranslate("Remove")}
                  </button>
                </div>
              ))}
              {query.data.votes.length === 0 && (
                <p className="empty-row muted">{deferredText("No vote channels yet.")}</p>
              )}
            </div>
            <PixelButton variant="secondary" onClick={() => setVote(newVote())}>
              {" "}
              {deferredText("Add vote channel")}{" "}
            </PixelButton>
          </SettingsRow>
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
                    setNotice(deferredText("Vote channel saved."));
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
        </div>
      )}
    </SettingsPage>
  );
}
