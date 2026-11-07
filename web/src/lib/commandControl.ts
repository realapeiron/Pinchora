import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
export interface CommandRule {
  enabled: boolean;
  allowed_roles: string[];
  denied_roles: string[];
  allowed_channels: string[];
  denied_channels: string[];
  cooldown_secs: number;
  delete_reply_secs: number;
  delete_invocation_secs: number;
  ephemeral: boolean;
}
export interface ControlConfig {
  rules: Record<string, CommandRule>;
  aliases: Record<string, string>;
}
export interface ControlCommand {
  path: string;
  description: string;
  permissions: string;
  ephemeral_supported: boolean;
  protected: boolean;
}
export interface ControlData {
  config: ControlConfig;
  commands: ControlCommand[];
  permissions: { id: string; permissions: { id: string; type: number; permission: boolean }[] }[] | null;
  permission_names: Record<string, string>;
  strings: Record<string, string>;
}
export const defaultRule = (): CommandRule => ({
  enabled: true,
  allowed_roles: [],
  denied_roles: [],
  allowed_channels: [],
  denied_channels: [],
  cooldown_secs: 0,
  delete_reply_secs: 0,
  delete_invocation_secs: 0,
  ephemeral: false,
});
export function filteredCommands(commands: ControlCommand[], search: string) {
  const needle = search.trim().toLocaleLowerCase();
  return commands.filter((c) => `${c.path} ${c.description}`.toLocaleLowerCase().includes(needle));
}
export function useCommandControl(guild: string) {
  const client = useQueryClient();
  const key = ["command-control", guild];
  const path = `/api/guilds/${guild}/command-control`;
  const query = useQuery({ queryKey: key, queryFn: () => api<ControlData>(path) });
  const save = useMutation({
    mutationFn: (config: ControlConfig) => api<ControlData>(path, "PUT", config),
    onSuccess: (data) => client.setQueryData(key, data),
  });
  return { query, save };
}
export interface BackupDocument {
  version: number;
  source_guild: string;
  names: { roles: Record<string, string>; channels: Record<string, string> };
  sections: Record<string, unknown[]>;
}
export interface BackupPreview {
  differences: { section: string; added: number; changed: number; removed: number }[];
  unmapped: string[];
  errors: string[];
  can_import: boolean;
}
export interface BackupData {
  points: { id: string; created_at: string }[];
  strings: Record<string, string>;
}
export type ImportRequest = { backup: BackupDocument; policy: "merge" | "replace" };
export function useBackup(guild: string) {
  const client = useQueryClient();
  const path = `/api/guilds/${guild}/backup`;
  const query = useQuery({ queryKey: ["backup", guild], queryFn: () => api<BackupData>(path) });
  const preview = useMutation({
    mutationFn: (request: ImportRequest) => api<BackupPreview>(`${path}/preview`, "POST", request),
  });
  const apply = useMutation({
    mutationFn: (request: ImportRequest) => api<{ message: string }>(`${path}/import`, "POST", request),
    onSuccess: () => client.invalidateQueries(),
  });
  const point = useMutation({ mutationFn: (id: string) => api<BackupDocument>(`${path}/restore/${id}`) });
  return { query, preview, apply, point, exportUrl: `${path}/export` };
}
