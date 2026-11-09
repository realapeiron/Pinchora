import {
  mkdtempSync,
  realpathSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
  symlinkSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import tseslint from "typescript-eslint";
import { ESLint } from "eslint";
import { expect, it } from "vitest";

const eslint = new ESLint();
const forms = [
  'const path = "../moderation/api/automod"; void import(path);',
  'const name = "automod"; void import(`../moderation/api/${name}`);',
  'void import("../moderation/api/" + "automod");',
  'import "../moderation/api/automod";',
  'import "@/features/moderation/api/automod";',
  'import "/src/features/moderation/api/automod";',
  'void import("../moderation/api/automod");',
  "void import(`../moderation/api/automod`);",
  'export { words } from "../moderation/api/automod";',
  'export * from "../moderation/api/automod";',
  'import type { Words } from "../moderation/api/automod"; export type { Words };',
  'export type Words = import("../moderation/api/automod").Words;',
];
it.each(forms)("rejects cross-feature internals: %s", async (code) => {
  for (const name of ["probe.ts", "probe.test.ts"]) {
    const [result] = await eslint.lintText(code, { filePath: `src/features/admin/${name}` });
    expect(result!.errorCount).toBe(1);
    expect(result!.messages.filter((message) => message.ruleId === "local/feature-boundaries")).toHaveLength(
      1,
    );
  }
});
it.each(["../moderation", "../moderation/index", "../moderation/index.ts", "./api/utility", "../../lib/api"])(
  "allows public/shared/same-feature imports: %s",
  async (path) => {
    const [result] = await eslint.lintText(`import "${path}";`, { filePath: "src/features/admin/probe.ts" });
    expect(result!.messages).toEqual([]);
  },
);

it("resolves real tsconfig path aliases before enforcing the boundary", async () => {
  const directory = realpathSync(mkdtempSync(join(tmpdir(), "feature-boundaries-")));
  try {
    mkdirSync(join(directory, "scripts"));
    mkdirSync(join(directory, "src/features/admin"), { recursive: true });
    mkdirSync(join(directory, "src/features/moderation/api"), { recursive: true });
    writeFileSync(join(directory, "src/features/moderation/api/automod.ts"), "export const words = {};");
    writeFileSync(
      join(directory, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          baseUrl: ".",
          paths: { "@features/*": ["src/features/*"] },
        },
      }),
    );
    symlinkSync(resolve("node_modules"), join(directory, "node_modules"), "dir");
    const ruleFile = join(directory, "scripts/feature-boundaries.mjs");
    copyFileSync(resolve("scripts/feature-boundaries.mjs"), ruleFile);
    const { default: rule } = await import(pathToFileURL(ruleFile).href);
    const aliasLint = new ESLint({
      cwd: directory,
      overrideConfigFile: true,
      overrideConfig: [
        ...tseslint.configs.recommended,
        {
          plugins: { local: { rules: { "feature-boundaries": rule } } },
          rules: { "local/feature-boundaries": "error" },
        },
      ],
    });
    const [result] = await aliasLint.lintText('import "@features/moderation/api/automod";', {
      filePath: join(directory, "src/features/admin/probe.test.ts"),
    });
    expect(result!.errorCount).toBe(1);
    expect(result!.messages[0]!.ruleId).toBe("local/feature-boundaries");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
