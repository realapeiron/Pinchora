import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import ts from "typescript";

import { date, number, plural, serverText, setLocale, translate } from ".";
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
    for (const key of Object.keys(en)) {
      const placeholders = (text: string) => [...text.matchAll(/\{\w+\}/g)].map(([value]) => value).sort();
      expect(placeholders(tr[key as keyof typeof tr]), `Placeholder mismatch: ${key}`).toEqual(
        placeholders(en[key as keyof typeof en]),
      );
    }
    const source =
      sourceFiles(join(process.cwd(), "src"))
        .map((file) => readFileSync(file, "utf8"))
        .join("\n") +
      readFileSync(join(process.cwd(), "src/lib/commands.json"), "utf8") +
      readFileSync(join(process.cwd(), "src/lib/rules.json"), "utf8") +
      readFileSync(join(process.cwd(), "../crates/pinchora-web/src/routes/community_pack.rs"), "utf8") +
      readFileSync(join(process.cwd(), "../crates/pinchora-web/src/routes/house_edit.rs"), "utf8") +
      readFileSync(join(process.cwd(), "../crates/pinchora-commands/src/house_edit.rs"), "utf8") +
      readFileSync(join(process.cwd(), "../crates/pinchora-commands/src/commands/house.rs"), "utf8") +
      ["modules", "logging", "moderation", "server_stats"]
        .map((name) => readFileSync(join(process.cwd(), `../crates/pinchora-core/src/${name}.rs`), "utf8"))
        .join("\n") +
      readdirSync(join(process.cwd(), "../crates/pinchora-core/locales/en"))
        .filter((name) => name.endsWith(".toml"))
        .map((name) => readFileSync(join(process.cwd(), "../crates/pinchora-core/locales/en", name), "utf8"))
        .join("\n");
    // These two are looked up from API status codes, so they do not appear as literal calls.
    const runtimeKeys = new Set(["appealed", "Messages must contain 1–200 characters."]);
    // TOML and TypeScript sources can spell multiline messages with escaped newlines.
    expect(
      Object.keys(en).filter(
        (key) =>
          !source.includes(key) &&
          !source.includes(JSON.stringify(key).slice(1, -1)) &&
          !runtimeKeys.has(key),
      ),
    ).toEqual([]);
    const missing: string[] = [];
    for (const file of sourceFiles(join(process.cwd(), "src"))) {
      const tree = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true);
      const visit = (node: ts.Node): void => {
        if (
          ts.isCallExpression(node) &&
          ["translate", "spaTranslate"].includes(node.expression.getText(tree)) &&
          node.arguments[0] &&
          ts.isStringLiteral(node.arguments[0]) &&
          !(node.arguments[0].text in en)
        ) {
          missing.push(`${file}: ${node.arguments[0].text}`);
        }
        ts.forEachChild(node, visit);
      };
      visit(tree);
    }
    expect(missing).toEqual([]);
  }, 20000);

  it("formats interpolation, Turkish counts, dates, and numbers", async () => {
    await setLocale("tr");
    expect(translate("{name}, {count} items", { name: "Ada", count: 3 }, "tr")).toBe("Ada, 3 eşya");
    expect(plural(3, { one: "1 unsaved change", other: "{count} unsaved changes" })).toBe(
      "3 kaydedilmemiş değişiklik",
    );
    expect(number(1234.5, "tr")).toBe("1.234,5");
    expect(date("2026-11-07T12:00:00Z", "tr")).toBe("7 Kas 2026");
    expect(serverText("title", { title: tr.BIRTHDAYS }, {}, "tr")).toBe(tr.BIRTHDAYS);
    await setLocale("en");
    expect(serverText("title", { title: tr.BIRTHDAYS }, {}, "tr")).toBe("BIRTHDAYS");
  });

  it("has no hard-coded English in JSX text or user-facing attributes", () => {
    const allowed = new Set([
      "Pinchora",
      "PINCHORA",
      "Pinchora Reef",
      "Discord",
      "GitHub",
      "MEE6",
      "FAQ",
      "XP",
      "KB",
      "px",
      "Mira",
      "Captain Crab",
      "&amp;",
    ]);
    const visibleAttributes = new Set([
      "aria-label",
      "title",
      "placeholder",
      "alt",
      "label",
      "hint",
      "none",
      "lead",
      "empty",
      "eyebrow",
      "caption",
      "intro",
      "note",
      "unit",
    ]);
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
        let copy: string | undefined;
        if (ts.isJsxText(node)) copy = node.getText(source).replace(/\s+/g, " ").trim();
        if (ts.isJsxAttribute(node) && visibleAttributes.has(node.name.getText(source))) {
          const value = node.initializer;
          if (value && ts.isStringLiteral(value)) copy = value.text;
          if (value && ts.isJsxExpression(value) && value.expression && ts.isStringLiteral(value.expression))
            copy = value.expression.text;
        }
        if (copy) copy = copy.trim();
        if (copy && /[a-z]{2}/i.test(copy) && !allowed.has(copy) && !/^\/[a-z]/i.test(copy))
          untranslated.push(`${file}: ${copy}`);
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(untranslated.length, untranslated.slice(0, 20).join("\n")).toBe(0);
  });
});
