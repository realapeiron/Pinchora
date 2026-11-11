import type { GuildInfo } from "../../../lib/api";
import { t } from "../../../i18n/dashboard";
import type { Board, StarboardData, VoteChannel } from "../api/starboard";
import { channelName } from "./starboardChannel";

export function starboardSummary(data: StarboardData) {
  return {
    boards: data.boards.length
      ? t(data.boards.length === 1 ? "{count} board" : "{count} boards", { count: data.boards.length })
      : t("No boards yet"),
    votes: data.votes.length
      ? t(data.votes.length === 1 ? "{count} vote channel" : "{count} vote channels", {
          count: data.votes.length,
        })
      : t("No vote channels yet."),
  };
}
export function boardSummary(board: Board, info: GuildInfo | undefined) {
  return {
    details: board.channel_id
      ? `${channelName(info, board.channel_id)} · ${board.threshold} ${t("stars")}`
      : t("No channel"),
    exclusions: `${board.only_channels.length} ${t("included")} · ${board.except_channels.length} ${t("excluded")}`,
    options: board.enabled ? t("On") : t("Off"),
    weights: Object.keys(board.role_weights).length
      ? t(Object.keys(board.role_weights).length === 1 ? "{count} weighted role" : "{count} weighted roles", {
          count: Object.keys(board.role_weights).length,
        })
      : t("No weighted roles"),
  };
}
export function voteSummary(vote: VoteChannel, info: GuildInfo | undefined) {
  return vote.channel_id
    ? `${channelName(info, vote.channel_id)} · ${vote.up_emoji} ${vote.down_emoji}`
    : t("No channel");
}
