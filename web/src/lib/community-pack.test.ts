import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { en } from "../i18n/en";
import { tr } from "../i18n/tr";
import { questionText, type Question } from "./community-pack";

describe("Community validation translations", () => {
  it("translates every server-side validation message into Turkish", () => {
    const source = readFileSync(
      new URL("../../../crates/pinchora-web/src/routes/community_pack.rs", import.meta.url),
      "utf8",
    );
    const messages = [...source.matchAll(/bad\(\s*(?:"[^"]+"|field),\s*"([^"]+)"/g)].map((match) => match[1]);
    messages.push("This server already has 10 forms.");
    expect(messages.length).toBeGreaterThan(10);
    for (const message of messages) {
      expect(en[message as keyof typeof en]).toBe(message);
      expect(tr[message as keyof typeof tr]).toBeTruthy();
      expect(tr[message as keyof typeof tr]).not.toBe(message);
    }
  });
});

it("shows bundled questions in the selected language and preserves suggestions", () => {
  const question: Question = { id: "1", text: "Question?", text_tr: "Soru?", approved: true, asked_at: null };
  expect(questionText(question, "en")).toBe("Question?");
  expect(questionText(question, "tr")).toBe("Soru?");
  expect(questionText({ ...question, text_tr: null }, "tr")).toBe("Question?");
});
