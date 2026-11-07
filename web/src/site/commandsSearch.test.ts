import { describe, expect, it } from "vitest";

import catalogJson from "../lib/commands.json";
import { type Catalog, type CommandArea, countCommands, filterAreas, terms } from "./commandsSearch";

const catalog = catalogJson as Catalog;

const areas: CommandArea[] = [
  {
    key: "fun",
    title: "Fun",
    intro: "",
    commands: [
      {
        path: "fish cast",
        usage: "/fish cast [spot]",
        description: "Cast a line and reel in a catch",
        manager: false,
        options: [
          { name: "spot", description: "Where to fish", required: false, choices: ["Shore", "Reef"] },
        ],
      },
      {
        path: "give",
        usage: "/give <user> <amount>",
        description: "Send shells to a friend",
        manager: false,
        options: [],
      },
    ],
  },
  {
    key: "setup",
    title: "Setup",
    intro: "",
    commands: [
      {
        path: "setup welcome message",
        usage: "/setup welcome message",
        description: "Welcome cards (Manage Server)",
        manager: true,
        options: [],
      },
    ],
  },
];

describe("terms", () => {
  it("lower-cases, drops leading slashes and empty words", () => {
    expect(terms("  /Fish   CAST ")).toEqual(["fish", "cast"]);
    expect(terms("")).toEqual([]);
  });
});

describe("filterAreas", () => {
  it("returns everything for an empty search", () => {
    expect(filterAreas(areas, "", false)).toBe(areas);
  });

  it("keeps the commands that match every word, wherever the word is", () => {
    expect(filterAreas(areas, "fish", false).map((a) => a.key)).toEqual(["fun"]);
    expect(countCommands(filterAreas(areas, "cast line", false))).toBe(1);
    expect(countCommands(filterAreas(areas, "reef", false))).toBe(1);
    expect(countCommands(filterAreas(areas, "shells friend", false))).toBe(1);
    expect(countCommands(filterAreas(areas, "fish friend", false))).toBe(0);
  });

  it("matches the usage and the area's name", () => {
    expect(countCommands(filterAreas(areas, "/give", false))).toBe(1);
    expect(countCommands(filterAreas(areas, "setup", false))).toBe(1);
  });

  it("drops areas that end up empty and can show only manager commands", () => {
    expect(filterAreas(areas, "zzz", false)).toEqual([]);
    const managers = filterAreas(areas, "", true);
    expect(managers.map((a) => a.key)).toEqual(["setup"]);
    expect(countCommands(filterAreas(areas, "welcome", true))).toBe(1);
    expect(countCommands(filterAreas(areas, "fish", true))).toBe(0);
  });
});

describe("the committed command list", () => {
  it("has every command once, with words, in areas that have anchors of their own", () => {
    expect(catalog.areas.length).toBeGreaterThan(8);
    expect(countCommands(catalog.areas)).toBe(catalog.count);
    const paths = catalog.areas.flatMap((a) => a.commands.map((c) => c.path));
    expect(new Set(paths).size).toBe(paths.length);
    expect(new Set(catalog.areas.map((a) => a.key)).size).toBe(catalog.areas.length);
    for (const area of catalog.areas) {
      expect(area.key).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
      expect(area.commands.length, area.title).toBeGreaterThan(0);
      for (const c of area.commands) {
        expect(c.description.length, c.path).toBeGreaterThan(3);
        if (c.path === "Report") {
          expect(c.usage).toBe("Message → Apps → Report");
        } else {
          expect(c.usage.startsWith(`/${c.path}`), c.path).toBe(true);
        }
      }
    }
  });

  it("finds the everyday commands", () => {
    const found = (query: string) =>
      filterAreas(catalog.areas, query, false).flatMap((a) => a.commands.map((c) => c.path));
    expect(found("/profile")).toContain("profile");
    expect(found("fish")).toContain("fish cast");
    expect(found("welcome")).toContain("setup welcome message");
  });

  it("marks a command that needs Manage Server", () => {
    const managers = filterAreas(catalog.areas, "", true).flatMap((a) => a.commands);
    expect(managers.length).toBeGreaterThan(10);
    expect(managers.some((c) => c.path === "setup welcome message")).toBe(true);
    expect(managers.some((c) => c.path === "profile")).toBe(false);
  });
});

it("finds canonical commands by their old names without duplicate slash entries", () => {
  const commands = catalog.areas.flatMap((a) => a.commands);
  expect(commands).toHaveLength(357);
  expect(new Set(commands.map((c) => c.path.split(" ")[0])).size).toBe(66);
  for (const command of commands) {
    for (const alias of command.aliases ?? []) {
      expect(filterAreas(catalog.areas, alias, false).flatMap((a) => a.commands)).toContain(command);
      expect(commands.some((c) => c.path === alias)).toBe(false);
    }
  }
});

it("includes every quick game in the canonical catalog", () => {
  const paths = catalog.areas.flatMap((a) => a.commands.map((c) => c.path));
  for (const path of [
    "play memory",
    "play minesweeper",
    "play higherlower",
    "play anagram",
    "play typing",
    "play guessnumber",
    "duel othello",
    "party nhie",
    "party fun",
  ]) {
    expect(paths).toContain(path);
  }
});
