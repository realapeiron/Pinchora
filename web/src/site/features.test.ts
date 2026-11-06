import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import catalogJson from "../lib/commands.json";
import facts from "../lib/facts.json";
import { type Catalog } from "./commandsSearch";
import { FEATURES, featureBySlug } from "./features";
import { ICONS, ICON_SIZE, PALETTE, runs } from "./icons";
import { SHOTS, altOf } from "./shots";

const catalog = catalogJson as Catalog;

describe("features", () => {
  it("are six areas with their own address", () => {
    expect(FEATURES.length).toBe(6);
    expect(new Set(FEATURES.map((f) => f.slug)).size).toBe(6);
    for (const f of FEATURES) expect(f.slug).toMatch(/^[a-z]+(-[a-z]+)*$/);
    expect(featureBySlug("games")?.title).toBe("Games");
    expect(featureBySlug("nothing")).toBeUndefined();
  });

  it("each say what they do, with points, pictures and a command to try", () => {
    for (const f of FEATURES) {
      expect(f.blurb.length, f.slug).toBeGreaterThan(20);
      expect(f.lead.length, f.slug).toBeGreaterThan(40);
      expect(f.points.length, f.slug).toBeGreaterThanOrEqual(6);
      expect(f.shots.length, f.slug).toBeGreaterThan(0);
      expect(f.try.length, f.slug).toBeGreaterThan(0);
      for (const s of f.shots) {
        expect(s.alt.length, `${f.slug} ${s.key}`).toBeGreaterThan(30);
        expect(SHOTS[s.key], s.key).toBeDefined();
      }
    }
  });

  it("only send people to commands that exist", () => {
    const paths = catalog.areas.flatMap((a) => a.commands.map((c) => c.path));
    for (const f of FEATURES) {
      for (const command of f.try.filter((c) => c.startsWith("/"))) {
        const path = command.slice(1);
        expect(
          paths.some((p) => p === path || p.startsWith(`${path} `)),
          `${f.slug}: ${command} is not a command`,
        ).toBe(true);
      }
    }
  });

  it("quote the counts the bot's content has, not a stale number", () => {
    const text = JSON.stringify(FEATURES);
    const counts: [RegExp, number][] = [
      [/(\d[\d,]*) (?:profile )?backgrounds/g, facts.backgrounds],
      [/(\d[\d,]*) badges/g, facts.badges],
      [/(\d[\d,]*) furniture designs/g, facts.furniture],
      [/(\d[\d,]*) pet species/g, facts.pets],
      [/(\d[\d,]*) (?:species of )?fish\b/g, facts.fish],
    ];
    let quoted = 0;
    for (const [pattern, expected] of counts) {
      for (const match of text.matchAll(pattern)) {
        quoted += 1;
        expect(Number((match[1] ?? "").replace(/,/g, "")), match[0]).toBe(expected);
      }
    }
    expect(quoted, "the copy quotes at least the backgrounds, badges and furniture").toBeGreaterThanOrEqual(
      3,
    );
  });

  it("describe every picture the same wherever it is used", () => {
    for (const key of Object.keys(SHOTS) as (keyof typeof SHOTS)[]) {
      expect(altOf(key).length, key).toBeGreaterThan(30);
    }
  });
});

describe("pictures", () => {
  /** The width and height a PNG says it has (its IHDR chunk). */
  function pngSize(file: string): [number, number] {
    const bytes = readFileSync(fileURLToPath(new URL(`./showcase/${file}.png`, import.meta.url)));
    expect(bytes.subarray(1, 4).toString("ascii"), file).toBe("PNG");
    return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
  }

  it("are as big as the page says, so nothing jumps when they load", () => {
    for (const [key, shot] of Object.entries(SHOTS)) {
      expect([shot.width, shot.height], key).toEqual(pngSize(key));
    }
  });
});

describe("pixel icons", () => {
  it("are twelve by twelve and use only colours of the palette", () => {
    for (const [name, rows] of Object.entries(ICONS)) {
      expect(rows.length, name).toBe(ICON_SIZE);
      for (const row of rows) {
        expect(row.length, `${name}: ${row}`).toBe(ICON_SIZE);
        for (const c of row) expect(c === "." || c in PALETTE, `${name}: ${c}`).toBe(true);
      }
    }
  });

  it("draw as runs that cover every coloured pixel once", () => {
    for (const [name, rows] of Object.entries(ICONS)) {
      const coloured = rows.join("").replace(/\./g, "").length;
      expect(
        runs(rows).reduce((sum, r) => sum + r.w, 0),
        name,
      ).toBe(coloured);
    }
  });
});
