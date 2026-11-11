import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = fileURLToPath(new URL("../", import.meta.url));

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

describe("SettingsRow IDs", () => {
  it("uses descriptive unique rowId string literals within each source file", () => {
    for (const file of sourceFiles(source)) {
      const ids = [...readFileSync(file, "utf8").matchAll(/rowId="([^"]+)"/g)].map((match) => match[1]!);
      expect(
        ids.filter((id) => /-\d+$/.test(id)),
        file,
      ).toEqual([]);
      expect(new Set(ids).size, file).toBe(ids.length);
    }
  });
});
