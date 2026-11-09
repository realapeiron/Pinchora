import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

it("keeps shared navigation styles eager and House overrides before the app stylesheet", () => {
  const main = readFileSync(new URL("./main.tsx", import.meta.url), "utf8");
  const house = readFileSync(new URL("./features/house/pages/House.tsx", import.meta.url), "utf8");
  const imports = [...main.matchAll(/import "\.\/styles\/([^"]+)"/g)].map((match) => match[1]);
  expect(imports).toEqual(["house.css", "tokens.css", "app.css", "site.css"]);
  expect(house).not.toContain('import "../../../styles/house.css"');
});
