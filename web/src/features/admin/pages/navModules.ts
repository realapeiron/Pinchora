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
