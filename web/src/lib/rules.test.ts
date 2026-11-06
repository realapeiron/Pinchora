import { describe, expect, it } from "vitest";

import rules from "./rules.json";
import {
  type Check,
  type MessageKind,
  checkAccent,
  checkLevel,
  checkMessage,
  checkMultiplier,
  checkPoints,
  checkPointsEmoji,
  checkPointsName,
  checkPrice,
  checkReputation,
  checkStock,
  checkTitle,
  checkVoiceAmount,
} from "./rules";
import { checkCommandName } from "./tags";

type Input = unknown;

const checks: Record<string, (input: Input) => Check<unknown>> = {
  accent: (s) => checkAccent(s as string),
  message: (i) => {
    const { kind, text } = i as { kind: MessageKind; text: string };
    return checkMessage(kind, text);
  },
  title: (s) => checkTitle(s as string),
  points_name: (s) => checkPointsName(s as string),
  points_emoji: (s) => checkPointsEmoji(s as string),
  points: (pair) => {
    const [min, max] = pair as [number, number];
    return checkPoints(min, max);
  },
  multiplier: (n) => checkMultiplier(n as number),
  voice_xp_amount: (n) => checkVoiceAmount(n as number),
  level: (n) => checkLevel(n as number),
  reputation: (n) => checkReputation(n as number),
  price: (n) => checkPrice(n as number),
  stock: (n) => checkStock(n as number),
  command_name: (s) => checkCommandName(s as string),
};

describe("the browser's checks answer like the server's", () => {
  it("covers every kind of sample", () => {
    const kinds = new Set(rules.cases.map((c) => c.check));
    expect([...kinds].sort()).toEqual(Object.keys(checks).sort());
  });

  for (const sample of rules.cases) {
    it(`${sample.check} ${JSON.stringify(sample.input).slice(0, 40)}`, () => {
      const check = checks[sample.check]!;
      expect(check(sample.input)).toEqual(sample.result);
    });
  }
});
