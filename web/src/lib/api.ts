// The dashboard's API client: JSON in and out, the CSRF token on every write, errors as ApiError.

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly field?: string;
  readonly inviteUrl?: string;

  constructor(status: number, code: string, message: string, field?: string, inviteUrl?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.field = field;
    this.inviteUrl = inviteUrl;
  }
}

let csrfToken: string | null = null;

/** Remembers the token `GET /api/me` handed out; every write sends it back. */
export function setCsrfToken(token: string | null): void {
  csrfToken = token;
}

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export async function api<T>(path: string, method: Method = "GET", body?: unknown): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
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
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  const json: unknown = text ? safeParse(text) : undefined;
  if (!response.ok) {
    const error = (json as { error?: Record<string, string> } | undefined)?.error;
    throw new ApiError(
      response.status,
      error?.code ?? "http_" + response.status,
      error?.message ?? `The server answered ${response.status}.`,
      error?.field,
      error?.invite_url,
    );
  }
  return json as T;
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

export interface Settings {
  leveling_enabled: boolean;
  levelup_mode: string;
  voice_xp_enabled: boolean;
  [field: string]: unknown;
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

export type SettingsPatch = Partial<Pick<Settings, "leveling_enabled">>;
