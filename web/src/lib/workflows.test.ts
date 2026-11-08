import { describe, expect, it } from "vitest";
import { blank, move, payload, actions, conditions } from "./workflows";
describe("workflow editor", () => {
  it("sends only editable fields", () => {
    const def = blank();
    expect(payload({ ...def, id: "12", runs: 88 } as typeof def)).toEqual(def);
    expect(def.enabled).toBe(false);
  });
  it("reorders without mutation or losing steps", () => {
    const source = [1, 2, 3];
    expect(move(source, 0, 1)).toEqual([2, 1, 3]);
    expect(source).toEqual([1, 2, 3]);
    expect(move(source, 0, -1)).toEqual(source);
  });
  it("offers the bounded F13 condition and action sets without wait, shells or workflow recursion", () => {
    expect(Object.keys(conditions)).toHaveLength(10);
    for (const forbidden of ["wait", "give_shells", "run_workflow", "llm"])
      expect(actions[forbidden]).toBeUndefined();
    expect(actions.give_points).toEqual({ kind: "tag", action: { kind: "give_points", amount: 1 } });
  });
});
