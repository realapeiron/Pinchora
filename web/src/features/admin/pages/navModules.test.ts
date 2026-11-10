import { readFileSync } from "node:fs";
import { expect, test } from "vitest";
import { NAV } from "./guildNav";
import { modulePage, navModule } from "./navModules";

test("every sidebar route has a deliberate module mapping with a catalog key", () => {
  const source = readFileSync(
    new URL("../../../../../crates/pinchora-core/src/admin/modules/catalog.rs", import.meta.url),
    "utf8",
  );
  const keyBlock =
    source.split("pub const fn key(self) -> &'static str {")[2]?.split("pub fn parse")[0] ?? "";
  const keys = new Set([...keyBlock.matchAll(/Self::\w+ => "([^"]+)"/g)].map((match) => match[1]));
  const paths = NAV.flatMap((group) => group.pages.map((page) => page.to));
  expect(keys.size).toBe(55);
  for (const path of paths) {
    const key = navModule(path);
    if (key) expect(keys.has(key), `${key} must be in the module catalog`).toBe(true);
  }
  expect(navModule("commands")).toBe("custom_commands");
  expect(navModule("temp-voice")).toBe("temp_voice");
  expect(navModule("command-control")).toBe("command_control");
  expect(navModule("roles")).toBeUndefined();
});

test("catalog cards open the page that holds their main setting", () => {
  const pages: Record<string, string> = {
    branding: "branding",
    command_control: "command-control",
    stats: "stats",
    invites: "invites",
    utility: "utility",
    welcome: "welcome",
    leveling: "leveling",
    points: "points",
    economy: "economy",
    custom_commands: "commands",
    workflows: "workflows",
    moderation: "moderation",
    automod: "automod",
    anti_nuke: "anti-nuke",
    log: "log",
    tickets: "tickets",
    events: "events",
    temp_voice: "temp-voice",
    starboard: "starboard",
    birthdays: "birthdays",
    giveaways: "giveaways",
    polls: "polls",
    suggestions: "suggestions",
    feeds: "feeds",
    roblox: "roblox",
    game_trackers: "trackers",
    achievements: "achievements",
    goodbye: "welcome",
    store: "points",
    role_panels: "roles",
    autoroles: "roles",
    color_roles: "roles",
    verification: "safety",
    anti_raid: "safety",
    text_commands: "",
  };
  for (const [key, page] of Object.entries(pages)) expect(modulePage(key)).toBe(page);
  const modulesOnly = [
    "profile",
    "shop",
    "quests",
    "social",
    "house",
    "plaza",
    "pets",
    "fishing",
    "duels",
    "board_games",
    "puzzles",
    "party",
    "party_games",
    "collection",
    "gambling",
    "info",
    "qotd",
    "counting",
    "forms",
    "modmail",
  ];
  for (const key of modulesOnly) expect(modulePage(key)).toBeUndefined();
  const source = readFileSync(
    new URL("../../../../../crates/pinchora-core/src/admin/modules/catalog.rs", import.meta.url),
    "utf8",
  );
  const keyBlock =
    source.split("pub const fn key(self) -> &'static str {")[2]?.split("pub fn parse")[0] ?? "";
  const catalogKeys = [...keyBlock.matchAll(/Self::\w+ => "([^"]+)"/g)].map((match) => match[1]);
  expect([...Object.keys(pages), ...modulesOnly].sort()).toEqual(catalogKeys.sort());
});
