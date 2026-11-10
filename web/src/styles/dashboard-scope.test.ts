import { readFileSync } from "node:fs";
import { expect, test } from "vitest";

/** A selector list split at its top-level commas; commas inside :is(), :has() and the like stay. */
function splitList(list: string): string[] {
  const parts: string[] = [];
  let depth = 0,
    start = 0;
  [...list].forEach((char, at) => {
    if (char === "(") depth += 1;
    else if (char === ")") depth -= 1;
    else if (char === "," && depth === 0) {
      parts.push(list.slice(start, at));
      start = at + 1;
    }
  });
  return [...parts, list.slice(start)].map((part) => part.trim());
}

/** Every selector in a stylesheet, at any nesting depth, without at-rule preludes. */
function selectors(css: string): string[] {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...bare.matchAll(/([^{};]+)\{/g)]
    .map((match) => (match[1] ?? "").trim())
    .filter((prelude) => !prelude.startsWith("@"))
    .flatMap(splitList);
}

test("dashboard styles stay inside the guild shell", () => {
  for (const file of ["dashboard-components.css", "dashboard-shell.css"]) {
    const css = readFileSync(new URL(file, import.meta.url), "utf8");
    const unscoped = selectors(css).filter((selector) => !selector.startsWith(":where(.shell-page)"));
    expect(unscoped, `${file} has unscoped selectors`).toEqual([]);
  }
});

test("the scope check sees nested and listed selectors", () => {
  const css =
    "@media (max-width: 960px) { :where(.shell-page) .a:has(b, i), .b { color: red } } /* .c { } */";
  expect(selectors(css)).toEqual([":where(.shell-page) .a:has(b, i)", ".b"]);
});
