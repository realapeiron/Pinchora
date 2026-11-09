import { describe, expect, it } from "vitest";

import {
  type Limits,
  checkPanel,
  duration,
  exampleName,
  listQuery,
  newPanel,
  panelBody,
  personLabel,
  stars,
} from "./tickets";

const LIMITS: Limits = {
  panels: 10,
  buttons: 5,
  options: 25,
  questions: 5,
  support_roles: 10,
  name: 32,
  title: 100,
  description: 2000,
  label: 80,
  option_description: 100,
  welcome: 1500,
  question: 45,
  placeholder: 100,
  answer: 1000,
  naming: 60,
  auto_close_hours: 720,
  open_per_user: 10,
};

describe("ticket panels", () => {
  it("a new panel is ready to save once it has a name", () => {
    const panel = newPanel("7001");
    expect(checkPanel(panel, LIMITS).map((e) => e.field)).toEqual(["name"]);
    expect(checkPanel({ ...panel, name: "Support" }, LIMITS)).toEqual([]);
  });

  it("names the problems the server would", () => {
    const panel = { ...newPanel(null), name: "12", title: " ", color: "red" };
    expect(checkPanel(panel, LIMITS).map((e) => e.field)).toEqual(["name", "channel_id", "title", "color"]);
    const six = Array.from({ length: 6 }, (_, n) => ({ label: `T${n}`, tone: "primary" as const }));
    const buttons = { ...newPanel("1"), name: "x", topics: six };
    expect(checkPanel(buttons, LIMITS)).toEqual([
      { field: "buttons", message: "This style shows at most 5 topics. Use a menu for more." },
    ]);
    expect(checkPanel({ ...buttons, style: "select" }, LIMITS)).toEqual([]);
    const questions = {
      ...newPanel("1"),
      name: "x",
      questions: [{ label: "", placeholder: "", long: false, required: true }],
    };
    expect(checkPanel(questions, LIMITS)[0]?.message).toBe("Question 1 needs a label.");
  });

  it("sends only what the API takes, trimmed", () => {
    const body = panelBody({
      ...newPanel("7001"),
      name: "  Help ",
      topics: [{ label: " Billing ", emoji: " ", description: "", tone: "danger" }],
    });
    expect(body.name).toBe("Help");
    expect(body.topics).toEqual([
      { label: "Billing", emoji: undefined, description: undefined, tone: "danger" },
    ]);
    expect(body.channel_id).toBe("7001");
  });
});

describe("ticket helpers", () => {
  it("shows the channel name a template makes", () => {
    expect(exampleName("ticket-{number}")).toBe("ticket-0042");
    expect(exampleName("{topic} {user}!!")).toBe("billing-sam");
    expect(exampleName("!!!")).toBe("");
  });

  it("says how long a first reply took", () => {
    expect(duration(null)).toBe("—");
    expect(duration(20)).toBe("under a minute");
    expect(duration(240)).toBe("4 min");
    expect(duration(7_500)).toBe("2 h 5 min");
    expect(duration(3 * 86_400)).toBe("3 d");
  });

  it("draws ratings and people", () => {
    expect(stars(4)).toBe("★★★★☆");
    expect(stars(9)).toBe("★★★★★");
    expect(personLabel(null)).toBe("—");
    expect(personLabel({ id: "5", name: null })).toBe("User 5");
    expect(personLabel({ id: "5", name: "Sam" })).toBe("Sam");
  });

  it("builds the list query from the filter", () => {
    expect(listQuery({ state: "", q: "", user: "", staff: "" }, "")).toBe("");
    expect(listQuery({ state: "closed", q: " bill ", user: "1", staff: "" }, "99")).toBe(
      "?state=closed&q=bill&user=1&before=99",
    );
  });
});
