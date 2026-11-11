import { expect, test } from "vitest";
import type { GuildInfo, ModuleEntry } from "../../../lib/api";
import { moduleSummary } from "./moduleSummary";

const module: ModuleEntry = {
  key: "pets",
  label: "Pets",
  blurb: "",
  group: "play",
  enabled: true,
  server_only: false,
  channel_mode: "all",
  channels: [],
};
const info = {
  channels: [
    { id: "1", name: "general", type: 0 },
    { id: "2", name: "games", type: 0 },
  ],
} as GuildInfo;

test("module rows summarise the effective channel scope and off state", () => {
  expect(moduleSummary(module, undefined, info)).toBe("All channels");
  expect(moduleSummary(module, { enabled: false }, info)).toBe("Off");
  expect(moduleSummary({ ...module, server_only: true }, undefined, info)).toBe("Whole server");
  expect(moduleSummary(module, { channel_mode: "only", channels: ["1", "2"] }, info)).toBe(
    "Only in: #general, #games",
  );
  expect(moduleSummary(module, { channel_mode: "except", channels: ["2"] }, info)).toBe(
    "Everywhere but: #games",
  );
  expect(moduleSummary(module, { channel_mode: "only", channels: [] }, info)).toBe("Only in: no channels");
});
