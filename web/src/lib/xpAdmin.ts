import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "./api";

export type XpMember = { id: string; name: string; xp: number };
export type ImportJob = {
  id: string;
  status: "running" | "done" | "failed";
  checked: number;
  updated: number;
  total: number;
  message: string | null;
};

export function useXpMembers(guild: string, search: string) {
  return useQuery({
    queryKey: ["xp-members", guild, search],
    queryFn: () =>
      api<{ items: XpMember[] }>(
        `/api/guilds/${guild}/xp-admin/members?search=${encodeURIComponent(search)}`,
      ),
    enabled: search.trim().length > 0,
  });
}

export function useEditXp(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      user,
      operation,
      amount,
    }: {
      user: string;
      operation: "add" | "remove" | "set";
      amount: number;
    }) => api<XpMember>(`/api/guilds/${guild}/xp-admin/members/${user}`, "PUT", { operation, amount }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["xp-members", guild] }),
  });
}

export function useImportMee6(guild: string) {
  return useMutation({
    mutationFn: (mode: "merge" | "replace") =>
      api<{ job_id: string }>(`/api/guilds/${guild}/xp-admin/import/mee6`, "POST", { mode }),
  });
}

export function useImportJob(guild: string, id: string | null) {
  return useQuery({
    queryKey: ["xp-import-job", guild, id],
    queryFn: () => api<ImportJob>(`/api/guilds/${guild}/xp-admin/jobs/${id}`),
    enabled: !!id,
    refetchInterval: (query) => (query.state.data?.status === "running" ? 800 : false),
  });
}
