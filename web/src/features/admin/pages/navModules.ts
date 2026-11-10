const switchPages = new Set([
  "branding",
  "command-control",
  "stats",
  "invites",
  "utility",
  "welcome",
  "leveling",
  "points",
  "economy",
  "commands",
  "workflows",
  "moderation",
  "automod",
  "anti-nuke",
  "log",
  "tickets",
  "events",
  "temp-voice",
  "starboard",
  "birthdays",
  "giveaways",
  "polls",
  "suggestions",
  "feeds",
  "roblox",
  "trackers",
  "achievements",
]);

const aliases: Record<string, string> = {
  "command-control": "command_control",
  commands: "custom_commands",
  "anti-nuke": "anti_nuke",
  "temp-voice": "temp_voice",
  trackers: "game_trackers",
};

export function navModule(path: string) {
  return switchPages.has(path) ? (aliases[path] ?? path) : undefined;
}

export function modulePage(key: string) {
  if (key === "goodbye") return "welcome";
  if (key === "store") return "points";
  if (key === "role_panels" || key === "autoroles" || key === "color_roles") return "roles";
  if (key === "verification" || key === "anti_raid") return "safety";
  if (key === "text_commands") return "";
  return [...switchPages].find((path) => navModule(path) === key);
}
