import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { BackupData } from "../api/commandControl";
import { backupSummary } from "./backupSummary";

test("backup rows describe empty files and restore point counts", async () => {
  await setLocale("en");
  expect(backupSummary({ points: [], strings: {} }, "")).toEqual({
    export: "Ready to export",
    import: "No file selected",
    points: "No restore points",
  });
  expect(
    backupSummary(
      {
        points: [
          { id: "1", created_at: "" },
          { id: "2", created_at: "" },
        ],
        strings: {},
      } as BackupData,
      "settings.json",
    ).points,
  ).toBe("2 restore points");
});
