import { locale, translate as spaTranslate } from "../i18n";
// Tickets: the shapes `/api/guilds/{g}/tickets/...` answers with, the hooks that read and write them, and
// the small rules the page checks before the server does (the server's sentences are the final word).

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { type FieldError, api } from "./api";

export type PanelStyle = "buttons" | "select";
export type Tone = "primary" | "secondary" | "success" | "danger";
export type Mode = "auto" | "channel" | "thread";
export type TicketState = "open" | "claimed" | "closed";

export interface Topic {
  label: string;
  emoji?: string | null;
  description?: string | null;
  tone: Tone;
}

export interface Question {
  label: string;
  placeholder: string;
  long: boolean;
  required: boolean;
}

export interface TicketSettings {
  open_category_id: string | null;
  closed_category_id: string | null;
  log_channel_id: string | null;
  auto_close_hours: number;
  dm_transcript: boolean;
  rating_enabled: boolean;
  naming: string;
  mode: Mode;
  max_open_per_user: number;
}

/** A panel as the page edits it; `id` is null until it is saved. */
export interface PanelDraft {
  name: string;
  channel_id: string | null;
  title: string;
  description: string;
  color: string;
  style: PanelStyle;
  topics: Topic[];
  questions: Question[];
  support_roles: string[];
  welcome_text: string;
  open_category_id: string | null;
  log_channel_id: string | null;
  enabled: boolean;
}

export interface Panel extends PanelDraft {
  id: string;
  channel_id: string;
  message_id: string | null;
  updated_at: string;
}

export interface Limits {
  panels: number;
  buttons: number;
  options: number;
  questions: number;
  support_roles: number;
  name: number;
  title: number;
  description: number;
  label: number;
  option_description: number;
  welcome: number;
  question: number;
  placeholder: number;
  answer: number;
  naming: number;
  auto_close_hours: number;
  open_per_user: number;
}

export interface Summary {
  opened: number;
  closed: number;
  open_now: number;
  median_first_reply_secs: number | null;
  ratings: number;
  rating_avg: number | null;
}

export interface TicketsOverview {
  settings: TicketSettings;
  panels: Panel[];
  limits: Limits;
  message_content: boolean;
  summary: { week: Summary; month: Summary };
}

export interface Person {
  id: string;
  name: string | null;
}

export interface Ticket {
  id: string;
  number: number;
  state: TicketState;
  topic: string;
  panel: string | null;
  opener: Person;
  claimed_by: Person | null;
  closed_by: Person | null;
  close_reason: string | null;
  channel_id: string | null;
  thread_id: string | null;
  locked: boolean;
  opened_at: string;
  closed_at: string | null;
  first_reply_at: string | null;
  rating: number | null;
  answers: { question: string; answer: string }[];
}

export interface TicketPage {
  items: Ticket[];
  next: string | null;
}

export interface TranscriptLine {
  /** The author, as an index into `authors`. */
  a: number;
  t: string;
  c?: string;
  f?: string[];
  e?: number;
}

export interface TranscriptAuthor {
  idx: number;
  name: string | null;
  bot: boolean;
}

export interface TicketDetail {
  ticket: Ticket;
  transcript: {
    with_content: boolean;
    messages: number;
    authors: TranscriptAuthor[];
    lines: TranscriptLine[];
  } | null;
  message_content: boolean;
}

export interface TicketFilter {
  state: "" | TicketState;
  q: string;
  user: string;
  staff: string;
}

// /Data

const base = (guild: string) => `/api/guilds/${guild}/tickets`;

export function useTickets(guild: string) {
  return useQuery({ queryKey: [base(guild)], queryFn: () => api<TicketsOverview>(base(guild)) });
}

function useRefresh(guild: string) {
  const client = useQueryClient();
  return async () => {
    await client.invalidateQueries({ queryKey: [base(guild)] });
    await client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] });
  };
}

export function useSaveTicketSettings(guild: string) {
  const client = useQueryClient();
  const refresh = useRefresh(guild);
  return useMutation({
    mutationFn: (settings: TicketSettings) => api<TicketSettings>(`${base(guild)}/settings`, "PUT", settings),
    onSuccess: async (settings) => {
      client.setQueryData<TicketsOverview>([base(guild)], (old) => (old ? { ...old, settings } : old));
      await refresh();
    },
  });
}

/** The body a panel is saved with: what the API takes, nothing more. */
export function panelBody(draft: PanelDraft) {
  return {
    name: draft.name.trim(),
    channel_id: draft.channel_id ?? "",
    title: draft.title,
    description: draft.description,
    color: draft.color,
    style: draft.style,
    topics: draft.topics.map((t) => ({
      label: t.label.trim(),
      emoji: t.emoji?.trim() ? t.emoji.trim() : undefined,
      description: t.description?.trim() ? t.description.trim() : undefined,
      tone: t.tone,
    })),
    questions: draft.questions.map((q) => ({ ...q, label: q.label.trim() })),
    support_roles: draft.support_roles,
    welcome_text: draft.welcome_text,
    open_category_id: draft.open_category_id,
    log_channel_id: draft.log_channel_id,
    enabled: draft.enabled,
  };
}

export function useSavePanel(guild: string) {
  const refresh = useRefresh(guild);
  return useMutation({
    mutationFn: ({ id, draft }: { id: string | null; draft: PanelDraft }) =>
      id
        ? api<Panel>(`${base(guild)}/panels/${id}`, "PUT", panelBody(draft))
        : api<Panel>(`${base(guild)}/panels`, "POST", panelBody(draft)),
    onSuccess: refresh,
  });
}

export function useDeletePanel(guild: string) {
  const refresh = useRefresh(guild);
  return useMutation({
    mutationFn: (id: string) => api<void>(`${base(guild)}/panels/${id}`, "DELETE"),
    onSuccess: refresh,
  });
}

export function usePostPanel(guild: string) {
  const refresh = useRefresh(guild);
  return useMutation({
    mutationFn: (id: string) =>
      api<{ channel_id: string; message_id: string; updated: boolean }>(
        `${base(guild)}/panels/${id}/post`,
        "POST",
      ),
    onSuccess: refresh,
  });
}

/** The list's query string for a filter and a page. */
export function listQuery(filter: TicketFilter, before: string): string {
  const params = new URLSearchParams();
  if (filter.state) params.set("state", filter.state);
  if (filter.q.trim()) params.set("q", filter.q.trim());
  if (filter.user.trim()) params.set("user", filter.user.trim());
  if (filter.staff.trim()) params.set("staff", filter.staff.trim());
  if (before) params.set("before", before);
  const text = params.toString();
  return text ? `?${text}` : "";
}

export function useTicketList(guild: string, filter: TicketFilter) {
  return useInfiniteQuery({
    queryKey: [`${base(guild)}/list`, filter],
    queryFn: ({ pageParam }) => api<TicketPage>(`${base(guild)}/list${listQuery(filter, pageParam)}`),
    initialPageParam: "",
    getNextPageParam: (last) => last.next ?? undefined,
  });
}

export function useTicket(guild: string, id: string | undefined) {
  return useQuery({
    queryKey: [`${base(guild)}/ticket/${id}`],
    queryFn: () => api<TicketDetail>(`${base(guild)}/ticket/${id}`),
    enabled: !!id,
  });
}

export function useCloseTicket(guild: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      api<{ id: string; state: "closed" }>(`${base(guild)}/ticket/${id}/close`, "POST"),
    onSuccess: async (_, id) => {
      await Promise.all([
        client.invalidateQueries({ queryKey: [`${base(guild)}/ticket/${id}`] }),
        client.invalidateQueries({ queryKey: [`${base(guild)}/list`] }),
        client.invalidateQueries({ queryKey: [base(guild)] }),
        client.invalidateQueries({ queryKey: [`/api/guilds/${guild}/audit`] }),
      ]);
    },
  });
}

export function transcriptUrl(guild: string, id: string): string {
  return `${base(guild)}/ticket/${id}/transcript.html`;
}

// /Rules

export const DEFAULT_COLOR = "#ff6b4a";

/** A new panel: one topic, buttons, posted in `channel`. */
export function newPanel(channel: string | null): PanelDraft {
  return {
    name: "",
    channel_id: channel,
    title: "Need help?",
    description: "Press the button below and a private ticket opens, just for you and the staff.",
    color: DEFAULT_COLOR,
    style: "buttons",
    topics: [{ label: "Open a ticket", emoji: "🎫", description: null, tone: "primary" }],
    questions: [],
    support_roles: [],
    welcome_text: "",
    open_category_id: null,
    log_channel_id: null,
    enabled: true,
  };
}

export function draftOf(panel: Panel): PanelDraft {
  return {
    name: panel.name,
    channel_id: panel.channel_id,
    title: panel.title,
    description: panel.description,
    color: panel.color,
    style: panel.style,
    topics: panel.topics.map((t) => ({ ...t, tone: t.tone ?? "primary" })),
    questions: panel.questions,
    support_roles: panel.support_roles,
    welcome_text: panel.welcome_text,
    open_category_id: panel.open_category_id,
    log_channel_id: panel.log_channel_id,
    enabled: panel.enabled,
  };
}

const chars = (text: string) => [...text].length;

/** The problems the server would name, checked as the manager types; the server checks again. */
export function checkPanel(draft: PanelDraft, limits: Limits): FieldError[] {
  const errors: FieldError[] = [];
  const name = draft.name.trim();
  if (!name) errors.push({ field: "name", message: "Give the panel a name." });
  else if (chars(name) > limits.name)
    errors.push({
      field: "name",
      message: spaTranslate("A panel's name is at most {arg1} characters.", { arg1: limits.name }),
    });
  else if (/^\d+$/.test(name))
    errors.push({ field: "name", message: "A panel's name can't be only a number." });
  if (!draft.channel_id) errors.push({ field: "channel_id", message: "Pick the channel it is posted in." });
  if (!draft.title.trim()) errors.push({ field: "title", message: "Give the panel a title." });
  else if (chars(draft.title) > limits.title)
    errors.push({
      field: "title",
      message: spaTranslate("A title is at most {arg1} characters.", { arg1: limits.title }),
    });
  if (chars(draft.description) > limits.description)
    errors.push({
      field: "description",
      message: spaTranslate("The text is at most {arg1} characters.", { arg1: limits.description }),
    });
  if (!/^#?[0-9a-fA-F]{6}$/.test(draft.color.trim()))
    errors.push({ field: "color", message: "Colours look like `#ff6b4a`." });
  const most = draft.style === "buttons" ? limits.buttons : limits.options;
  if (draft.topics.length === 0)
    errors.push({
      field: "buttons",
      message: "Add at least one topic: it becomes the button members press.",
    });
  else if (draft.topics.length > most)
    errors.push({
      field: "buttons",
      message: spaTranslate("This style shows at most {arg1} topics. Use a menu for more.", { arg1: most }),
    });
  draft.topics.forEach((t, n) => {
    if (!t.label.trim())
      errors.push({
        field: "buttons",
        message: spaTranslate("Topic {arg1} needs a label.", { arg1: n + 1 }),
      });
    else if (chars(t.label.trim()) > limits.label)
      errors.push({
        field: "buttons",
        message: spaTranslate("Topic {arg1}'s label is at most {arg2} characters.", {
          arg1: n + 1,
          arg2: limits.label,
        }),
      });
  });
  if (chars(draft.welcome_text) > limits.welcome)
    errors.push({
      field: "welcome_text",
      message: spaTranslate("The welcome text is at most {arg1} characters.", { arg1: limits.welcome }),
    });
  if (draft.questions.length > limits.questions)
    errors.push({
      field: "questions",
      message: spaTranslate("A form asks at most {arg1} questions.", { arg1: limits.questions }),
    });
  draft.questions.forEach((q, n) => {
    if (!q.label.trim())
      errors.push({
        field: "questions",
        message: spaTranslate("Question {arg1} needs a label.", { arg1: n + 1 }),
      });
    else if (chars(q.label.trim()) > limits.question)
      errors.push({
        field: "questions",
        message: spaTranslate("Question {arg1} is at most {arg2} characters.", {
          arg1: n + 1,
          arg2: limits.question,
        }),
      });
  });
  return errors;
}

/** `ticket-{number}` with an example filled in, as the channel would be called. */
export function exampleName(template: string, number = 42, user = "sam", topic = "billing"): string {
  const filled = template
    .replaceAll("{number}", String(number).padStart(4, "0"))
    .replaceAll("{user}", user)
    .replaceAll("{topic}", topic)
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^\p{L}\p{N}_-]/gu, "")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  return filled.slice(0, 100);
}

/** "4 min", "2 h 5 min", "3 d": how long a first reply took. */
export function duration(secs: number | null): string {
  if (secs === null || !Number.isFinite(secs)) return "—";
  const minutes = Math.round(secs / 60);
  if (minutes < 1) return spaTranslate("under a minute");
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
  return `${Math.round(hours / 24)} d`;
}

/** "★★★★☆". */
export function stars(rating: number): string {
  const full = Math.max(0, Math.min(5, Math.round(rating)));
  return "★".repeat(full) + "☆".repeat(5 - full);
}

export const STATE_LABEL: Record<TicketState, string> = {
  open: "Open",
  claimed: "Claimed",
  closed: "Closed",
};

/** A person as the table shows them: the name Discord has, else the id. */
export function personLabel(person: Person | null): string {
  if (!person) return "—";
  return person.name ?? spaTranslate("User {arg1}", { arg1: person.id });
}

/** A time as "6 Nov, 18:04", in the viewer's zone. */
export function when(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(locale() === "tr" ? "tr-TR" : "en-US", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
