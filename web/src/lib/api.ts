// The dashboard's API client: JSON in and out, the CSRF token on every write, errors as ApiError.

/** One field a form got wrong, with the sentence the bot would answer with. */
export interface FieldError {
  field: string;
  message: string;
}

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly field?: string;
  readonly inviteUrl?: string;
  /** Every field a 422 named; at least the one in `field`. */
  readonly errors: FieldError[];

  constructor(
    status: number,
    code: string,
    message: string,
    field?: string,
    inviteUrl?: string,
    errors?: FieldError[],
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.field = field;
    this.inviteUrl = inviteUrl;
    this.errors = errors?.length ? errors : field ? [{ field, message }] : [];
  }
}

let csrfToken: string | null = null;

/** Remembers the token `GET /api/me` handed out; every write sends it back. */
export function setCsrfToken(token: string | null): void {
  csrfToken = token;
}

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

async function send(path: string, method: Method, body: unknown, accept: string): Promise<Response> {
  const headers: Record<string, string> = { Accept: accept };
  if (method !== "GET") {
    headers["Content-Type"] = "application/json";
    if (csrfToken) headers["X-CSRF-Token"] = csrfToken;
  }
  const response = await fetch(path, {
    method,
    headers,
    credentials: "same-origin",
    body: method === "GET" ? undefined : JSON.stringify(body ?? {}),
  });
  if (!response.ok) throw await errorOf(response);
  return response;
}

async function errorOf(response: Response): Promise<ApiError> {
  const text = await response.text();
  const json: unknown = text ? safeParse(text) : undefined;
  const error = (json as { error?: Record<string, unknown> } | undefined)?.error;
  const field = (key: string) => (typeof error?.[key] === "string" ? (error[key] as string) : undefined);
  return new ApiError(
    response.status,
    field("code") ?? "http_" + response.status,
    field("message") ?? `The server answered ${response.status}.`,
    field("field"),
    field("invite_url"),
    Array.isArray(error?.errors) ? (error.errors as FieldError[]) : undefined,
  );
}

export async function api<T>(path: string, method: Method = "GET", body?: unknown): Promise<T> {
  const response = await send(path, method, body, "application/json");
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  return (text ? safeParse(text) : undefined) as T;
}

/** A POST that answers with a picture: the bytes as a Blob. */
export async function apiImage(path: string, body: unknown, signal?: AbortSignal): Promise<Blob> {
  const response = await send(path, "POST", body, "image/png");
  if (signal?.aborted) throw new DOMException("aborted", "AbortError");
  return response.blob();
}

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/** Whether an error means the user must log in (again). */
export function needsLogin(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}

// /Shapes the API answers with. Discord ids are strings.

export interface Me {
  user: { id: string; username: string; global_name: string | null; avatar_url: string };
  csrf: string;
}

export interface GuildEntry {
  id: string;
  name: string;
  icon_url: string | null;
  owner: boolean;
  bot_present: boolean;
  invite_url: string | null;
}

/** A server's settings row; ids are strings, the accent is `#rrggbb`, `null` means unset. */
export interface Settings {
  leveling_enabled: boolean;
  levelup_channel_id: string | null;
  levelup_message: string | null;
  levelup_mode: string;
  level_roles_stack: boolean;
  voice_xp_enabled: boolean;
  voice_xp_amount: number;
  welcome_enabled: boolean;
  welcome_channel_id: string | null;
  welcome_message: string | null;
  welcome_theme: string | null;
  welcome_title: string | null;
  welcome_accent: string | null;
  join_role_id: string | null;
  goodbye_channel_id: string | null;
  goodbye_message: string | null;
  points_enabled: boolean;
  points_name: string;
  points_emoji: string;
  points_min: number;
  points_max: number;
  log_channel_id: string | null;
  log_members: boolean;
  log_names: boolean;
  log_bans: boolean;
  log_voice: boolean;
}

export interface AuditEntry {
  id: string;
  actor_id: string;
  actor_name: string;
  source: "web" | "discord";
  action: string;
  target: string | null;
  before: Record<string, unknown> | null;
  after: Record<string, unknown> | null;
  created_at: string;
}

export interface Overview {
  guild: { id: string; name: string; icon_url: string | null };
  settings: Settings;
  audit: AuditEntry[];
}

export type SettingsPatch = Partial<Settings>;

/** A channel as the bot sees it; `type` is Discord's number (0 text, 2 voice, 4 category, 5 news). */
export interface Channel {
  id: string;
  name: string;
  type: number;
  parent_id: string | null;
  position: number;
}

export interface Role {
  id: string;
  name: string;
  color: number;
  position: number;
  managed: boolean;
  everyone: boolean;
  assignable: boolean;
  why_not: string | null;
}

export interface GuildInfo {
  channels: Channel[];
  roles: Role[];
  bot: { top_role_position: number | null; manage_roles: boolean };
}

export interface XpRule {
  kind: "channel" | "role";
  target_id: string;
  multiplier: number;
}

export interface XpRules {
  items: XpRule[];
  max: number;
}

export interface LevelRoles {
  items: { role_id: string; level: number }[];
  max: number;
  stack: boolean;
}

export interface RepRoles {
  items: { role_id: string; reputation: number }[];
  max: number;
}

export interface StoreItem {
  id: string;
  role_id: string;
  price: number;
  stock: number | null;
}

export interface Store {
  items: StoreItem[];
  max: number;
}

export interface Community {
  suggestions: { channel_id: string | null };
  critters: { enabled: boolean; channel_id: string | null };
}

export interface Job {
  id: string;
  status: "running" | "done" | "failed";
  checked: number;
  updated: number;
  total: number;
  message: string | null;
}

export interface AuditPage {
  items: AuditEntry[];
  next: string | null;
}
