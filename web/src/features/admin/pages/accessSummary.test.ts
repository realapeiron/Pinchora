import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import { accessSummary } from "./accessSummary";

test("access row describes empty and populated manager grants", async () => {
  await setLocale("en");
  expect(accessSummary([])).toBe("No manager roles");
  expect(accessSummary([{ role_id: "1", areas: [] }])).toBe("1 manager role");
});
