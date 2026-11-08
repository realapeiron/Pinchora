import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";

export type Curve =
  | "classic"
  | "mee6"
  | { linear: { base: number; per_level: number } }
  | { polynomial: { base: number; linear: number; quadratic: number } };
export type LevelingParity = {
  curve: Curve;
  message_min: number;
  message_max: number;
  message_cooldown_secs: number;
  messages: boolean;
  reactions: boolean;
  voice: boolean;
  effort: boolean;
  votes: boolean;
  separate_voice_level: boolean;
  reset_on_leave: boolean;
  leave_grace_secs: number;
  reaction_xp: number;
  effort_bonus: number;
  vote_xp: number;
  highlights_channel_id: string | null;
  vanity_slug: string | null;
};
export type ImportPreview = {
  preview_id: string;
  count: number;
  changed: number;
  sample: { user_id: string; xp: number; current_xp: number; result_xp: number }[];
};

export function useLevelingParity(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/leveling-parity`;
  const query = useQuery({
    queryKey: ["leveling-parity", guild],
    queryFn: () => api<LevelingParity>(path),
  });
  const save = useMutation({
    mutationFn: (value: LevelingParity) => api<LevelingParity>(path, "PUT", value),
    onSuccess: (value) => client.setQueryData(["leveling-parity", guild], value),
  });
  const highlights = useQuery({
    queryKey: ["leveling-highlights", guild],
    queryFn: () => api<{ week: string; items: { user_id: string; xp: number }[] }>(`${path}/highlights`),
  });
  const voiceRoles = useQuery({
    queryKey: ["voice-level-roles", guild],
    queryFn: () => api<{ items: { role_id: string; level: number }[]; max: number }>(`${path}/voice-roles`),
  });
  const putVoiceRole = useMutation({
    mutationFn: (value: { role: string; level: number }) =>
      api(`${path}/voice-roles/${value.role}`, "PUT", { level: value.level }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["voice-level-roles", guild] }),
  });
  const deleteVoiceRole = useMutation({
    mutationFn: (role: string) => api(`${path}/voice-roles/${role}`, "DELETE"),
    onSuccess: () => client.invalidateQueries({ queryKey: ["voice-level-roles", guild] }),
  });
  const preview = useMutation({
    mutationFn: (body: { format: "json" | "csv"; data: string; mode: "merge" | "replace" }) =>
      api<ImportPreview>(`${path}/import/preview`, "POST", body),
  });
  const apply = useMutation({
    mutationFn: (preview_id: string) =>
      api<{ checked: number; updated: number }>(`${path}/import/apply`, "POST", { preview_id }),
    onSuccess: () => {
      client.invalidateQueries({ queryKey: ["leveling-highlights", guild] });
      client.invalidateQueries({ queryKey: ["xp-members", guild] });
    },
  });
  return { query, save, highlights, voiceRoles, putVoiceRole, deleteVoiceRole, preview, apply };
}

export async function readLevelImport(file: Pick<File, "size" | "name" | "text">) {
  if (file.size > 2_000_000) throw new Error("File must be at most 2 MB.");
  return {
    data: await file.text(),
    format: file.name.toLowerCase().endsWith(".csv") ? ("csv" as const) : ("json" as const),
  };
}
