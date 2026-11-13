import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";
import ts from "typescript";

import {
  clearUser,
  configureGuild,
  configureUser,
  date,
  locale,
  number,
  plural,
  serverText,
  setLocale,
  translate,
} from ".";
import { t as dashboardText, dashboardEn, dashboardTr } from "./dashboard";
import { adminEn } from "./dashboard-admin-en";
import { adminTr } from "./dashboard-admin-tr";
import { communityEn } from "./dashboard-community-en";
import { communityTr } from "./dashboard-community-tr";
import { dashboardEn as baseDashboardEn } from "./dashboard-en";
import { dashboardTr as baseDashboardTr } from "./dashboard-tr";
import { settingsEn } from "./dashboard-settings-en";
import { settingsTr } from "./dashboard-settings-tr";
import { en } from "./en";
import { tr } from "./tr";
import { t as workflowText, workflowEn, workflowTr } from "./workflows";

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

function rustFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? rustFiles(path) : path.endsWith(".rs") ? [path] : [];
  });
}

describe("SPA message catalogs", () => {
  it("uses the manager's browser language before the guild language when no choice is saved", () => {
    vi.stubGlobal("navigator", { language: "en-US" });
    configureUser("unsaved-panel-language", null);
    configureGuild("tr");
    expect(locale()).toBe("en");
    clearUser();
    vi.unstubAllGlobals();
  });
  it("ignores an old local choice when the account has no saved panel choice", () => {
    vi.stubGlobal("navigator", { language: "en-US" });
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => (key === "pinchora.language.stale-panel-choice" ? "tr" : null),
    });
    configureUser("stale-panel-choice", null);
    expect(locale()).toBe("en");
    clearUser();
    vi.unstubAllGlobals();
  });
  it("keeps dashboard areas disjoint and complete in Turkish", () => {
    const areas: [Record<string, string>, Record<string, string>][] = [
      [baseDashboardEn, baseDashboardTr],
      [adminEn, adminTr],
      [communityEn, communityTr],
      [settingsEn, settingsTr],
    ];
    const seen = new Set<string>();
    for (const [english, turkish] of areas) {
      expect(Object.keys(turkish).sort()).toEqual(Object.keys(english).sort());
      for (const key of Object.keys(english)) {
        expect(seen.has(key), key).toBe(false);
        expect(key in en || key in tr, key).toBe(false);
        expect((turkish[key] ?? "").trim().length, key).toBeGreaterThan(0);
        seen.add(key);
      }
    }
  });
  it("uses singular and plural warning hints in both languages", async () => {
    await setLocale("en");
    expect(
      dashboardText("{member} has {count} active warning. Next: {action}.", {
        member: "Mia",
        count: 1,
        action: "timeout",
      }),
    ).toBe("Mia has 1 active warning. Next: timeout.");
    expect(
      dashboardText("{member} has {count} active warnings. Next: {action}.", {
        member: "Mia",
        count: 2,
        action: "timeout",
      }),
    ).toBe("Mia has 2 active warnings. Next: timeout.");
    await setLocale("tr");
    expect(
      dashboardText("{member} has {count} active warning. Next: {action}.", {
        member: "Mia",
        count: 1,
        action: "timeout",
      }),
    ).toBe("Mia için 1 aktif uyarı var. Sonraki eşik: timeout.");
    expect(
      dashboardText("{member} has {count} active warnings. Next: {action}.", {
        member: "Mia",
        count: 2,
        action: "timeout",
      }),
    ).toBe("Mia için 2 aktif uyarı var. Sonraki eşik: timeout.");
    await setLocale("en");
  });
  it("keeps deferred dashboard translations complete and switches languages", async () => {
    expect(Object.keys(dashboardTr).sort()).toEqual(Object.keys(dashboardEn).sort());
    for (const key of Object.keys(dashboardEn) as (keyof typeof dashboardEn)[]) {
      expect(key in en).toBe(false);
      expect(key in tr).toBe(false);
      expect(dashboardTr[key].trim().length, key).toBeGreaterThan(0);
      const placeholders = (text: string) => [...text.matchAll(/\{\w+\}/g)].map(([value]) => value).sort();
      expect(placeholders(dashboardTr[key]), key).toEqual(placeholders(dashboardEn[key]));
      await setLocale("tr");
      expect(dashboardText(key)).toBe(dashboardTr[key]);
      await setLocale("en");
      expect(dashboardText(key)).toBe(dashboardEn[key]);
    }
    await setLocale("tr");
    expect(dashboardText("{count} modules switched on", { count: 1234 })).toBe("1.234 modül açık");
    expect(dashboardText("Overview")).toBe(tr.Overview);
    await setLocale("en");
  });

  it("keeps deferred workflow translations complete in both languages", async () => {
    expect(Object.keys(workflowTr).sort()).toEqual(Object.keys(workflowEn).sort());
    for (const key of Object.keys(workflowEn) as (keyof typeof workflowEn)[]) {
      expect(workflowTr[key].trim().length, key).toBeGreaterThan(0);
      expect([...workflowTr[key].matchAll(/\{\w+\}/g)].map(([value]) => value).sort()).toEqual(
        [...workflowEn[key].matchAll(/\{\w+\}/g)].map(([value]) => value).sort(),
      );
      await setLocale("tr");
      expect(workflowText(key)).toBe(workflowTr[key]);
      await setLocale("en");
      expect(workflowText(key)).toBe(workflowEn[key]);
    }
  });

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
      readFileSync(join(process.cwd(), "../crates/pinchora-web/src/routes/community/engagement.rs"), "utf8") +
      readFileSync(
        join(process.cwd(), "../crates/pinchora-web/src/routes/community/tickets/operations.rs"),
        "utf8",
      ) +
      readFileSync(join(process.cwd(), "../crates/pinchora-web/src/routes/house/house_edit.rs"), "utf8") +
      readFileSync(join(process.cwd(), "../crates/pinchora-commands/src/house/house_edit.rs"), "utf8") +
      rustFiles(join(process.cwd(), "../crates/pinchora-commands/src/commands/house"))
        .map((file) => readFileSync(file, "utf8"))
        .join("\n") +
      ["admin/modules", "admin/logging", "moderation/moderation", "admin/server_stats"]
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
      Object.keys({ ...en, ...workflowEn, ...dashboardEn }).filter(
        (key) =>
          !source.includes(key) &&
          !source.includes(JSON.stringify(key).slice(1, -1)) &&
          !runtimeKeys.has(key),
      ),
    ).toEqual([]);
    const missing: string[] = [];
    for (const file of sourceFiles(join(process.cwd(), "src"))) {
      const code = readFileSync(file, "utf8");
      const dashboardNames = new Set<string>();
      const tree = ts.createSourceFile(file, code, ts.ScriptTarget.Latest, true);
      for (const statement of tree.statements) {
        if (
          ts.isImportDeclaration(statement) &&
          ts.isStringLiteral(statement.moduleSpecifier) &&
          statement.moduleSpecifier.text.endsWith("/i18n/dashboard") &&
          statement.importClause?.namedBindings &&
          ts.isNamedImports(statement.importClause.namedBindings)
        ) {
          for (const element of statement.importClause.namedBindings.elements) {
            if ((element.propertyName?.text ?? element.name.text) === "t")
              dashboardNames.add(element.name.text);
          }
        }
      }
      const visit = (node: ts.Node): void => {
        const callee = ts.isCallExpression(node) ? node.expression.getText(tree) : "";
        if (
          ts.isCallExpression(node) &&
          (["translate", "spaTranslate"].includes(callee) || dashboardNames.has(callee)) &&
          node.arguments[0] &&
          ts.isStringLiteral(node.arguments[0]) &&
          !(node.arguments[0].text in en) &&
          !(dashboardNames.has(callee) && node.arguments[0].text in dashboardEn)
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
