import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
export interface Node {
  kind: string;
  [key: string]: unknown;
}
export interface Definition {
  name: string;
  enabled: boolean;
  trigger: Node;
  conditions: Node[];
  actions: Node[];
}
export interface Workflow extends Definition {
  id: string;
  runs: number;
  last_error: string | null;
}
export interface Step {
  phase: string;
  index: number;
  status: string;
  detail: Node;
}
export interface Playback {
  outcome: string;
  steps: Step[];
}
export interface Run extends Playback {
  id: string;
  started_at: string;
}
export interface Data {
  items: Workflow[];
  triggers: Node[];
  message_content: boolean;
  max: number;
}
export const blank = (): Definition => ({
  name: "",
  enabled: false,
  trigger: { kind: "command" },
  conditions: [],
  actions: [{ kind: "reply", text: "", embed: null }],
});
export function payload(d: Definition): Definition {
  return {
    name: d.name,
    enabled: d.enabled,
    trigger: d.trigger,
    conditions: d.conditions,
    actions: d.actions,
  };
}
export function fingerprint(d: Definition): string {
  return JSON.stringify(payload(d), (_, value: unknown) =>
    value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)))
      : value,
  );
}
export function move<T>(items: T[], index: number, delta: number): T[] {
  const next = [...items];
  const target = index + delta;
  if (target < 0 || target >= items.length) return next;
  [next[index], next[target]] = [next[target]!, next[index]!];
  return next;
}
export const conditions: Record<string, Node> = {
  has_role: { kind: "has_role", role_id: "" },
  lacks_role: { kind: "lacks_role", role_id: "" },
  in_channel: { kind: "in_channel", channel_id: "" },
  min_level: { kind: "min_level", level: 1 },
  account_age: { kind: "account_age", days: 7 },
  chance: { kind: "chance", percent: 50 },
  cooldown: { kind: "cooldown", scope: "member", seconds: 60 },
  var_equals: { kind: "var_equals", scope: "server", key: "", value: "" },
  var_greater: { kind: "var_greater", scope: "server", key: "", value: 0 },
  time_window: { kind: "time_window", start: 540, end: 1020, tz: "UTC" },
};
export const actions: Record<string, Node> = {
  send_message: { kind: "send_message", channel_id: "", text: "", embed: null },
  reply: { kind: "reply", text: "", embed: null },
  ...Object.fromEntries(
    ["add_role", "remove_role", "toggle_role"].map((kind) => [
      kind,
      { kind: "tag", action: { kind, role_id: "" } },
    ]),
  ),
  dm: { kind: "tag", action: { kind: "dm", text: "" } },
  give_points: { kind: "tag", action: { kind: "give_points", amount: 1 } },
  take_points: { kind: "take_points", amount: 1 },
  set_var: { kind: "set_var", scope: "server", key: "", value: "" },
  incr_var: { kind: "incr_var", scope: "server", key: "", amount: 1 },
  create_thread: { kind: "create_thread", channel_id: "", name: "" },
  add_reaction: { kind: "add_reaction", emoji: "⭐" },
  pin: { kind: "pin" },
  open_ticket: { kind: "open_ticket", panel_id: 1, topic: 0 },
  log: { kind: "log", text: "" },
  random_branch: {
    kind: "random_branch",
    branches: [[{ kind: "log", text: "" }], [{ kind: "log", text: "" }]],
  },
};
export function useWorkflows(guild: string) {
  const client = useQueryClient();
  const key = ["workflows", guild];
  const path = `/api/guilds/${guild}/workflows`;
  const refresh = () => client.invalidateQueries({ queryKey: key });
  const query = useQuery({ queryKey: key, queryFn: () => api<Data>(path) });
  const save = useMutation({
    mutationFn: ({ id, def }: { id?: string; def: Definition }) =>
      api<Workflow>(`${path}${id ? `/${id}` : ""}`, id ? "PUT" : "POST", payload(def)),
    onSuccess: refresh,
  });
  const remove = useMutation({
    mutationFn: (id: string) => api(`${path}/${id}`, "DELETE"),
    onSuccess: refresh,
  });
  const test = useMutation({
    mutationFn: ({ definition, event }: { definition: Definition; event: Record<string, unknown> }) =>
      api<{ dry_run: boolean; playback: Playback }>(`${path}/test`, "POST", {
        definition: payload(definition),
        event,
      }),
  });
  return { query, save, remove, test, path };
}

// API-owned labels included in the catalogs; kinds are filtered by the server module state.
export const workflowEventKinds = [
  "command",
  "button",
  "member_join",
  "member_leave",
  "level_up",
  "role_added",
  "role_removed",
  "reaction_added",
  "message_keyword",
  "schedule",
  "voice_join",
  "birthday",
  "achievement",
  "ticket_opened",
  "ticket_closed",
  "starboard_post",
  "feed_item",
];
export const playbackStates = [
  "ready",
  "refused",
  "would_run",
  "skipped",
  "running",
  "done",
  "failed",
  "filtered",
  "completed",
];
