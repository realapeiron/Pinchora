import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import ts from "typescript";

import { date, number, plural, setLocale, translate } from ".";
import { en } from "./en";
import { tr } from "./tr";

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.name === "i18n") return [join(directory, "i18n/LanguageSwitcher.tsx")];
    const path = join(directory, entry.name);
    return entry.isDirectory()
      ? sourceFiles(path)
      : /\.tsx?$/.test(path) && !path.endsWith(".test.ts")
        ? [path]
        : [];
  });
}

describe("SPA message catalogs", () => {
  it("has a Turkish translation for every English key and no orphaned keys", () => {
    expect(Object.keys(tr).sort()).toEqual(Object.keys(en).sort());
    const source =
      sourceFiles(join(process.cwd(), "src"))
        .map((file) => readFileSync(file, "utf8"))
        .join("\n") + readFileSync(join(process.cwd(), "src/lib/commands.json"), "utf8");
    // These two are looked up from API status codes, so they do not appear as literal calls.
    const runtimeKeys = new Set(["appealed", "Messages must contain 1–200 characters."]);
    expect(Object.keys(en).filter((key) => !source.includes(key) && !runtimeKeys.has(key))).toEqual([]);
  });

  it("formats interpolation, Turkish counts, dates, and numbers", async () => {
    await setLocale("tr");
    expect(translate("{name}, {count} items", { name: "Ada", count: 3 }, "tr")).toBe("Ada, 3 eşya");
    expect(plural(3, { one: "1 unsaved change", other: "{count} unsaved changes" })).toBe(
      "3 kaydedilmemiş değişiklik",
    );
    expect(number(1234.5, "tr")).toBe("1.234,5");
    expect(date("2026-11-07T12:00:00Z", "tr")).toBe("7 Kas 2026");
    await setLocale("en");
  });

  it("has no hard-coded English in JSX text nodes", () => {
    const brands = new Set(["Pinchora", "PINCHORA", "Discord", "GitHub", "MEE6", "FAQ"]);
    const untranslated: string[] = [];
    for (const file of sourceFiles(join(process.cwd(), "src")).filter((path) => path.endsWith(".tsx"))) {
      const source = ts.createSourceFile(
        file,
        readFileSync(file, "utf8"),
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.TSX,
      );
      const visit = (node: ts.Node): void => {
        if (ts.isJsxText(node)) {
          const copy = node.getText(source).replace(/\s+/g, " ").trim();
          if (/[a-z]{2}/i.test(copy) && !brands.has(copy) && !/^\/[a-z]/i.test(copy))
            untranslated.push(`${file}: ${copy}`);
        }
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(untranslated.length, untranslated.slice(0, 20).join("\n")).toBe(0);
  });
});
