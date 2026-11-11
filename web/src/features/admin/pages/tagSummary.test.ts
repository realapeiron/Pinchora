import { expect, test } from "vitest";
import type { AutoResponder, CustomCommand } from "../../../lib/api";
import {
  commandItemSummary,
  commandListSummary,
  responderItemSummary,
  responderListSummary,
} from "./tagSummary";

const command = {
  enabled: true,
  responses: ["Hello\nthere"],
  embed: null,
  actions: [],
} as unknown as CustomCommand;
const responder = {
  enabled: true,
  responses: ["Welcome!"],
  embed: null,
  actions: [],
} as unknown as AutoResponder;

test("command rows summarise replies, disabled state and an empty list", () => {
  expect(commandItemSummary(command)).toBe("Hello there");
  expect(commandItemSummary({ ...command, enabled: false })).toBe("Off");
  expect(commandListSummary([])).toBe("No commands yet");
  expect(commandListSummary([command])).toBe("1 command");
});

test("responder rows summarise replies, disabled state and an empty list", () => {
  expect(responderItemSummary(responder)).toBe("Welcome!");
  expect(responderItemSummary({ ...responder, enabled: false })).toBe("Off");
  expect(responderListSummary([])).toBe("No responders yet");
  expect(responderListSummary([responder])).toBe("1 responder");
});
