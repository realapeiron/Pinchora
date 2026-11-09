import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../lib/api";

const base = (guild: string) => `/api/guilds/${guild}/tickets/operations`;

export interface SupportWindow {
  day: number;
  start: number;
  end: number;
}
export interface OperationSettings {
  support_timezone: string;
  support_hours: SupportWindow[];
  out_of_hours_message: string;
  status_channel_id: string | null;
  status_message_id: string | null;
  close_timeout_hours: number;
}
export interface PanelOperations {
  claim_mode: "optional" | "required" | "exclusive";
  max_open_per_panel: number;
  max_open_per_category: number;
  cooldown_seconds: number;
  minimum_account_age_days: number;
  minimum_membership_days: number;
  bypass_roles: string[];
  staff_open_limit: number;
  overflow_message: string;
  faq: { question: string; answer: string }[];
}
export interface TicketOperations {
  priority: string;
  escalated_at: string | null;
  transferred_to_user: string | null;
  transferred_to_role: string | null;
  close_requested_by: string | null;
  close_requested_at: string | null;
  close_request_expires_at: string | null;
  force_opened_by: string | null;
  last_reminder_at: string | null;
}
export interface StaffNote {
  id: string;
  author_id: string;
  body: string;
  created_at: string;
}
export interface Snippet {
  id: number;
  name: string;
  body: string;
}
export interface StaffScore {
  user_id: string;
  name: string | null;
  claimed: number;
  closed: number;
}

export function useTicketLeaderboard(guild: string) {
  return useQuery({
    queryKey: [base(guild), "leaderboard"],
    queryFn: () => api<{ items: StaffScore[] }>(`${base(guild)}/leaderboard`),
  });
}

export function useTicketOperationSettings(guild: string) {
  return useQuery({
    queryKey: [base(guild), "settings"],
    queryFn: () => api<OperationSettings>(`${base(guild)}/settings`),
  });
}

export function useSaveTicketOperationSettings(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: Omit<OperationSettings, "status_message_id">) =>
      api<OperationSettings>(`${base(guild)}/settings`, "PUT", body),
    onSuccess: () => client.invalidateQueries({ queryKey: [base(guild), "settings"] }),
  });
}

export function useTicketPanelOperations(guild: string, panel: string | null) {
  return useQuery({
    queryKey: [base(guild), "panel", panel],
    queryFn: () => api<PanelOperations>(`${base(guild)}/panel/${panel}`),
    enabled: !!panel,
  });
}

export function useSaveTicketPanelOperations(guild: string, panel: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: PanelOperations) => api<PanelOperations>(`${base(guild)}/panel/${panel}`, "PUT", body),
    onSuccess: () => client.invalidateQueries({ queryKey: [base(guild), "panel", panel] }),
  });
}

export function useTicketOperations(guild: string, ticket: string) {
  return useQuery({
    queryKey: [base(guild), "ticket", ticket, "operations"],
    queryFn: () => api<TicketOperations>(`${base(guild)}/ticket/${ticket}/operations`),
  });
}

export function useTicketNotes(guild: string, ticket: string) {
  return useQuery({
    queryKey: [base(guild), "ticket", ticket, "notes"],
    queryFn: () => api<{ items: StaffNote[] }>(`${base(guild)}/ticket/${ticket}/notes`),
  });
}

export function useTicketOperationAction(
  guild: string,
  ticket: string,
  action: "claim" | "unclaim" | "reply" | "notes",
) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (text?: string) =>
      api(`${base(guild)}/ticket/${ticket}/${action}`, "POST", text === undefined ? undefined : { text }),
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: [base(guild), "ticket", ticket] }),
        client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/tickets/ticket/${ticket}`] }),
        client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/tickets/list`] }),
        client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] }),
      ]);
    },
  });
}

export function useTicketSnippets(guild: string) {
  return useQuery({
    queryKey: [base(guild), "snippets"],
    queryFn: () => api<{ items: Snippet[] }>(`${base(guild)}/snippets`),
  });
}

export function useSaveTicketSnippet(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: { name: string; body: string }) =>
      api<Snippet>(`${base(guild)}/snippets`, "POST", body),
    onSuccess: () => client.invalidateQueries({ queryKey: [base(guild), "snippets"] }),
  });
}

export function useDeleteTicketSnippet(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api<void>(`${base(guild)}/snippets/${id}`, "DELETE"),
    onSuccess: () => client.invalidateQueries({ queryKey: [base(guild), "snippets"] }),
  });
}

export function snippetChanged(items: Snippet[], name: string, body: string): boolean {
  const cleanName = name.trim();
  const cleanBody = body.trim();
  return (
    !!cleanName && !!cleanBody && !items.some((item) => item.name === cleanName && item.body === cleanBody)
  );
}

export function operationSettingsBody(
  settings: OperationSettings,
): Omit<OperationSettings, "status_message_id"> {
  return {
    support_timezone: settings.support_timezone,
    support_hours: settings.support_hours,
    out_of_hours_message: settings.out_of_hours_message,
    status_channel_id: settings.status_channel_id,
    close_timeout_hours: settings.close_timeout_hours,
  };
}
