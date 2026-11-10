import type { ModuleEntry, Settings } from "../../../lib/api";
import { change, dayLabel, niceMax, type GuildStats } from "../api/stats";

export type CardKey = "messages" | "members" | "commands" | "active";
export interface StatCard {
  key: CardKey;
  value: string;
  change: number | null;
}

export function statCards(stats: GuildStats | undefined): StatCard[] | null {
  if (
    !stats?.module_on ||
    (!stats.daily.some((row) => row.messages || row.joins || row.leaves || row.commands) &&
      !stats.totals.active_members)
  )
    return null;
  const { totals, previous } = stats;
  return [
    { key: "messages", value: String(totals.messages), change: change(totals.messages, previous?.messages) },
    {
      key: "members",
      value: `${totals.joins} / ${totals.leaves}`,
      change: change(totals.joins - totals.leaves, previous ? previous.joins - previous.leaves : null),
    },
    { key: "commands", value: String(totals.commands), change: change(totals.commands, previous?.commands) },
    {
      key: "active",
      value: totals.active_members === null ? "—" : String(totals.active_members),
      change: change(totals.active_members ?? 0, previous?.active_members),
    },
  ];
}

export interface Signal {
  key: string;
  page: string;
}
export function attention(
  settings: Settings,
  modules: ModuleEntry[],
  roles: number | null,
  statsOn?: boolean,
): Signal[] {
  const on = (key: string) => modules.find((module) => module.key === key)?.enabled === true;
  const signals: Signal[] = [];
  if (on("welcome") && !settings.welcome_channel_id) signals.push({ key: "welcome", page: "welcome" });
  if (on("goodbye") && !settings.goodbye_channel_id) signals.push({ key: "goodbye", page: "welcome" });
  if (on("log") && !settings.log_channel_id) signals.push({ key: "log", page: "log" });
  if (
    statsOn === false ||
    (statsOn === undefined && modules.find((module) => module.key === "stats")?.enabled === false)
  )
    signals.push({ key: "stats", page: "stats" });
  if (on("leveling") && settings.leveling_enabled && roles === 0)
    signals.push({ key: "roles", page: "leveling" });
  return signals;
}

export interface SetupStep {
  key: string;
  page: string;
  done: boolean;
}
export function setupSteps(settings: Settings, modules: ModuleEntry[]): SetupStep[] {
  const on = (key: string) => modules.find((module) => module.key === key)?.enabled === true;
  return [
    { key: "welcome", page: "welcome", done: on("welcome") && !!settings.welcome_channel_id },
    { key: "goodbye", page: "welcome", done: on("goodbye") && !!settings.goodbye_channel_id },
    { key: "log", page: "log", done: on("log") && !!settings.log_channel_id },
    { key: "leveling", page: "leveling", done: on("leveling") && settings.leveling_enabled },
    { key: "stats", page: "stats", done: on("stats") },
  ];
}

export function catalogFilter(modules: ModuleEntry[], group: string, search: string, onlyOn: boolean) {
  const q = search.trim().toLocaleLowerCase();
  return modules.filter(
    (module) =>
      (group === "all" || module.group === group) &&
      (!onlyOn || module.enabled) &&
      (!q || `${module.label} ${module.blurb}`.toLocaleLowerCase().includes(q)),
  );
}

export function chartModel(rows: GuildStats["daily"]) {
  const max = niceMax(Math.max(1, ...rows.map((row) => row.messages)));
  return {
    max,
    bars: rows.map((row) => ({
      day: row.day,
      label: dayLabel(row.day),
      value: row.messages,
      accessible: `${dayLabel(row.day)}: ${row.messages}`,
    })),
    ticks: Array.from({ length: 5 }, (_, i) => (max * i) / 4),
  };
}
