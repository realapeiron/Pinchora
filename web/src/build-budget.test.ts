import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { expect, it } from "vitest";

it.each([250_000, 250_001])(
  "checks the actual main entry at %i bytes regardless of hash or manifest order",
  (bytes) => {
    const directory = mkdtempSync(join(tmpdir(), "main-budget-"));
    try {
      mkdirSync(join(directory, "dist/.vite"), { recursive: true });
      mkdirSync(join(directory, "dist/assets"));
      writeFileSync(
        join(directory, "dist/.vite/manifest.json"),
        JSON.stringify({
          "other.html": { isEntry: true, file: "assets/other.js" },
          "index.html": { isEntry: true, file: "assets/index-changed-hash.js" },
        }),
      );
      writeFileSync(join(directory, "dist/assets/other.js"), "");
      writeFileSync(join(directory, "dist/assets/index-changed-hash.js"), "x".repeat(bytes));
      const result = spawnSync(process.execPath, [resolve("scripts/check-main.mjs")], {
        cwd: directory,
        encoding: "utf8",
        timeout: 10_000,
      });
      expect(result.status).toBe(bytes > 250_000 ? 1 : 0);
      expect(result.stdout).toContain(`${bytes} / 250000`);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
