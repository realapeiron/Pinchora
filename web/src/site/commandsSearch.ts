// Searching the command list: the shapes in `web/src/lib/commands.json` and the filter the page runs.

export interface CommandOption {
  name: string;
  description: string;
  required: boolean;
  choices?: string[];
}

export interface CommandEntry {
  path: string;
  aliases?: string[];
  usage: string;
  description: string;
  /** Needs Manage Server (or a role that has it). */
  manager: boolean;
  options: CommandOption[];
}

export interface CommandArea {
  key: string;
  title: string;
  intro: string;
  commands: CommandEntry[];
}

export interface Catalog {
  count: number;
  areas: CommandArea[];
}

/** What a search is looking at in one command, lower case. */
function haystack(area: CommandArea, command: CommandEntry): string {
  const options = command.options.map((o) => `${o.name} ${o.description} ${(o.choices ?? []).join(" ")}`);
  return [command.usage, command.description, area.title, ...(command.aliases ?? []), ...options]
    .join(" ")
    .toLowerCase();
}

/** The words of a search: lower case, no leading slash, no empty ones. */
export function terms(query: string): string[] {
  return query
    .toLowerCase()
    .split(/\s+/)
    .map((t) => t.replace(/^\/+/, ""))
    .filter(Boolean);
}

/**
 * The areas with only the commands that match every word of `query` (and, when asked, only the ones for
 * managers). Areas left empty are dropped. A search with no words and no filter returns everything.
 */
export function filterAreas(areas: CommandArea[], query: string, managersOnly: boolean): CommandArea[] {
  const words = terms(query);
  if (words.length === 0 && !managersOnly) return areas;
  const out: CommandArea[] = [];
  for (const area of areas) {
    const commands = area.commands.filter((command) => {
      if (managersOnly && !command.manager) return false;
      const text = haystack(area, command);
      return words.every((w) => text.includes(w));
    });
    if (commands.length > 0) out.push({ ...area, commands });
  }
  return out;
}

export function countCommands(areas: CommandArea[]): number {
  return areas.reduce((sum, area) => sum + area.commands.length, 0);
}
