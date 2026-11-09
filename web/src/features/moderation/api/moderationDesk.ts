import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../lib/api";
export interface ModPolicy {
  report_channel: string | null;
  mute_role: string | null;
  jail_role: string | null;
  appeals: boolean;
  booster_role: string | null;
  booster_personal: boolean;
  autopurge: { channel: string; age_secs: number; bots_only: boolean }[];
  conditions: { role: string; requires: string[]; all: boolean; voice_channel: string | null }[];
}
export interface QueueRow {
  id: string;
  user_id: string;
  body: string;
  status: string;
  case_number?: number;
  reporter_id?: string;
  channel_id?: string;
  message_id?: string;
}
export interface ModerationDeskData {
  config: ModPolicy;
  queues: { reports: QueueRow[]; appeals: QueueRow[] };
}
export function useModerationDesk(guild: string) {
  const client = useQueryClient();
  const key = ["mod-parity", guild];
  const path = `/api/guilds/${guild}/mod-parity`;
  const query = useQuery({ queryKey: key, queryFn: () => api<ModerationDeskData>(path) });
  const save = useMutation({
    mutationFn: (policy: ModPolicy) => api<ModerationDeskData>(path, "PUT", policy),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  const preview = useMutation({
    mutationFn: (policy: ModPolicy) => api<{ valid: boolean }>(`${path}/preview`, "POST", policy),
  });
  const review = useMutation({
    mutationFn: ({
      queue,
      id,
      accept,
      reason,
    }: {
      queue: string;
      id: string;
      accept: boolean;
      reason: string;
    }) => api<ModerationDeskData>(`${path}/${queue}/${id}`, "POST", { accept, reason }),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  return { query, save, preview, review };
}
export function policyChanged(a: ModPolicy, b: ModPolicy) {
  return JSON.stringify(a) !== JSON.stringify(b);
}
