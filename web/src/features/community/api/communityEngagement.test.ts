import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { en } from "../../../i18n/en";
import { tr } from "../../../i18n/tr";
import { questionText, type Question } from "./communityEngagement";

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

it("sends counting settings without read-only counters", async () => {
  const { countingSettings } = await import("./communityEngagement");
  expect(
    countingSettings({
      channel_id: "7001",
      current: 42,
      record: 100,
      reset_on_fail: true,
      allow_math: false,
      fail_role_id: null,
    }),
  ).toEqual({
    channel_id: "7001",
    reset_on_fail: true,
    allow_math: false,
    fail_role_id: null,
  });
});

it("sends both new and existing application settings without stored IDs", async () => {
  const { applicationSettings } = await import("./communityEngagement");
  const settings = {
    name: "Helper",
    questions: ["Why help?"],
    submit_channel_id: "7001",
    review_roles: ["9001"],
    accept_role_id: null,
    dm_result: true,
    open: true,
  };
  for (const id of ["", "123"]) {
    expect(applicationSettings({ ...settings, id, message_id: "456" })).toEqual(settings);
  }
});
