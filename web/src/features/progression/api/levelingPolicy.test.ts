import { describe, expect, it, vi } from "vitest";
import { readLevelImport } from "./levelingPolicy";

describe("leveling import file validation", () => {
  it("rejects oversized files before reading their contents", async () => {
    const text = vi.fn().mockResolvedValue("id,xp\n1,5");
    await expect(readLevelImport({ size: 2_000_001, name: "export.csv", text })).rejects.toThrow("2 MB");
    expect(text).not.toHaveBeenCalled();
  });
  it("preserves snowflake text and recognizes CSV case-insensitively", async () => {
    const data = "id,xp\n123456789012345678,500";
    await expect(
      readLevelImport({ size: data.length, name: "export.CSV", text: async () => data }),
    ).resolves.toEqual({ data, format: "csv" });
  });
  it("does not return an import after a failed file read", async () => {
    await expect(
      readLevelImport({
        size: 20,
        name: "export.json",
        text: async () => {
          throw new Error("read failed");
        },
      }),
    ).rejects.toThrow("read failed");
  });
});
