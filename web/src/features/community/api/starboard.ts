import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "../../../lib/api";

export interface Board {
  id?: string;
  name: string;
  channel_id: string;
  emoji: string;
  threshold: number;
  self_star: boolean;
  allow_bots: boolean;
  only_channels: string[];
  except_channels: string[];
  require_image: boolean;
  color: string;
  role_weights: Record<string, number>;
  remove_below: boolean;
  enabled: boolean;
}
export interface VoteChannel {
  channel_id: string;
  up_emoji: string;
  down_emoji: string;
  only_media: boolean;
  create_thread: boolean;
  enabled: boolean;
}
export interface StarboardData {
  boards: Board[];
  votes: VoteChannel[];
  message_content: boolean;
  limits: { boards: number; threshold: number };
}
export const newBoard = (): Board => ({
  name: "Highlights",
  channel_id: "",
  emoji: "⭐",
  threshold: 3,
  self_star: false,
  allow_bots: false,
  only_channels: [],
  except_channels: [],
  require_image: false,
  color: "#ffc857",
  role_weights: {},
  remove_below: true,
  enabled: true,
});
export const newVote = (): VoteChannel => ({
  channel_id: "",
  up_emoji: "⬆️",
  down_emoji: "⬇️",
  only_media: false,
  create_thread: false,
  enabled: true,
});

export function useStarboard(guild: string) {
  const key = ["starboard", guild];
  const client = useQueryClient();
  const query = useQuery({
    queryKey: key,
    queryFn: () => api<StarboardData>(`/api/guilds/${guild}/starboard`),
  });
  const refresh = () => client.invalidateQueries({ queryKey: key });
  const saveBoard = useMutation({
    mutationFn: (body: Board) => api<Board>(`/api/guilds/${guild}/starboard/boards`, "POST", body),
    onSuccess: refresh,
  });
  const removeBoard = useMutation({
    mutationFn: (id: string) => api(`/api/guilds/${guild}/starboard/boards/${id}`, "DELETE"),
    onSuccess: refresh,
  });
  const saveVote = useMutation({
    mutationFn: (body: VoteChannel) => api<VoteChannel>(`/api/guilds/${guild}/starboard/votes`, "POST", body),
    onSuccess: refresh,
  });
  const removeVote = useMutation({
    mutationFn: (id: string) => api(`/api/guilds/${guild}/starboard/votes/${id}`, "DELETE"),
    onSuccess: refresh,
  });
  return { query, saveBoard, removeBoard, saveVote, removeVote };
}
