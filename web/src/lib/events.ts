import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";

export interface Event {
  id: string;
  title: string;
  description: string;
  starts_at: string;
  ends_at: string;
  tz: string;
  location: string;
  channel_id: string | null;
  required_role_id: string | null;
  options: { key: string; label: string; capacity: number }[];
  recurrence: { repeat: "daily" | "weekly"; until: string } | null;
  status: "upcoming" | "ended" | "cancelled";
  discord_event_id: string | null;
  warning?: string | null;
}
export interface EventBody {
  title: string;
  description: string;
  starts_at: string;
  ends_at: string;
  tz: string;
  location: string;
  channel_id: string | null;
  required_role_id: string | null;
  options: Event["options"];
  recurrence: Event["recurrence"];
  sync: boolean;
}
export interface EventData {
  language: string;
  strings: Record<string, string>;
  timezone: string;
  events: Event[];
}
export interface Attendees {
  count: number;
  attendees: { user_id: string; option: string; waitlisted: boolean; responded_at: string }[];
}
export function useEvents(guild: string) {
  const client = useQueryClient();
  const key = ["events", guild];
  const query = useQuery({ queryKey: key, queryFn: () => api<EventData>(`/api/guilds/${guild}/events`) });
  const refresh = () => client.invalidateQueries({ queryKey: key });
  const create = useMutation({
    mutationFn: (body: EventBody) => api<Event>(`/api/guilds/${guild}/events`, "POST", body),
    onSuccess: refresh,
  });
  const save = useMutation({
    mutationFn: ({ id, body }: { id: string; body: EventBody }) =>
      api<Event>(`/api/guilds/${guild}/events/${id}`, "PUT", body),
    onSuccess: refresh,
  });
  const cancel = useMutation({
    mutationFn: (id: string) => api<Event>(`/api/guilds/${guild}/events/${id}`, "DELETE"),
    onSuccess: refresh,
  });
  const timezone = useMutation({
    mutationFn: (value: string) => api(`/api/guilds/${guild}/events/timezone`, "POST", { timezone: value }),
    onSuccess: refresh,
  });
  const attendees = (id: string) => api<Attendees>(`/api/guilds/${guild}/events/${id}/attendees`);
  return { query, create, save, cancel, timezone, attendees };
}

/** A datetime-local control in the event's IANA time zone, independent of the viewer's computer. */
export function zonedInput(iso: string, zone: string): string {
  const parts = new Intl.DateTimeFormat("en-GB", {
    timeZone: zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(new Date(iso));
  const part = (key: string) => parts.find((p) => p.type === key)?.value ?? "00";
  return `${part("year")}-${part("month")}-${part("day")}T${part("hour")}:${part("minute")}`;
}
export function fromZonedInput(value: string, zone: string): string {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(value)) throw new Error("Choose a complete date and time.");
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  const hour = Number(value.slice(11, 13));
  const minute = Number(value.slice(14, 16));
  const target = Date.UTC(year, month - 1, day, hour, minute);
  let instant = target;
  for (let i = 0; i < 3; i++) {
    const shown = zonedInput(new Date(instant).toISOString(), zone);
    instant +=
      target -
      Date.UTC(
        Number(shown.slice(0, 4)),
        Number(shown.slice(5, 7)) - 1,
        Number(shown.slice(8, 10)),
        Number(shown.slice(11, 13)),
        Number(shown.slice(14, 16)),
      );
  }
  const iso = new Date(instant).toISOString();
  if (zonedInput(iso, zone) !== value) throw new Error("This local time does not exist in that time zone.");
  // Probe the offsets on both sides of a transition, including half-hour DST changes.
  for (const hours of [-24, -2, -1, 1, 2, 24]) {
    const probe = instant + hours * 3600000;
    const shown = zonedInput(new Date(probe).toISOString(), zone);
    const local = Date.parse(`${shown}:00Z`);
    const candidate = target - (local - probe);
    if (candidate !== instant && zonedInput(new Date(candidate).toISOString(), zone) === value)
      throw new Error("This local time is ambiguous in that time zone.");
  }
  return iso;
}
