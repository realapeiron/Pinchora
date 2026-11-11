import { expect, test } from "vitest";
import type { AutoConfig, AutoData, AutoRule } from "../api/automod";
import { automodRuleSummary, automodSummary } from "./automodSummary";

const config = { profile: "off", rules: [], escalation: [], alert_channel_id: null } as unknown as AutoConfig;
const data = { hits: [] } as unknown as AutoData;
const words = (key: string) => (key === "ui.off" ? "Off" : "Standard");

test("AutoMod summary handles off and empty data", () => {
  expect(automodSummary(config, data, words)).toEqual({
    profile: "Off",
    rules: "0 rules",
    settings: "Default alert channel · 0 escalation steps",
    dm: "Off",
    test: "Preview only",
    hits: "No recent hits",
  });
});

test("AutoMod summary follows profile, rules, alerts, steps and hits", () => {
  const value = {
    ...config,
    profile: "standard",
    dm_on_action: true,
    rules: [{}, {}],
    escalation: [{}],
    alert_channel_id: "123",
  } as AutoConfig;
  expect(automodSummary(value, { ...data, hits: [{}, {}, {}] } as AutoData, words)).toMatchObject({
    profile: "Standard",
    rules: "2 rules",
    settings: "Alert channel selected · 1 escalation steps",
    dm: "On",
    hits: "3 recent hits",
  });
});

test("AutoMod rule summary handles off, empty, dry run and actions", () => {
  const rule = { enabled: false, log_only: false, actions: [] } as unknown as AutoRule;
  const ruleWords = (key: string) => ({ "ui.dry_result": "Dry run", "action.ban": "Ban" })[key] ?? key;
  expect(automodRuleSummary(rule, words)).toBe("Off");
  expect(automodRuleSummary({ ...rule, enabled: true }, words)).toBe("No actions");
  expect(automodRuleSummary({ ...rule, enabled: true, log_only: true }, ruleWords)).toBe("Dry run");
  expect(
    automodRuleSummary(
      { ...rule, enabled: true, actions: [{ kind: "ban", secs: 0, role: null }] },
      ruleWords,
    ),
  ).toBe("Ban");
});
