import { expect, test } from "vitest";
import type { GuildInfo } from "../../../lib/api";
import type { AutoRole, Panel, RolesView } from "../api/roles";
import {
  autoRolesSummary,
  colorRolesSummary,
  rolePanelSummary,
  rolePanelsSummary,
  stickyRolesSummary,
} from "./roleSummary";

const panel = { title: "Games", style: "buttons", roles: [{ role_id: "1" }], channel_id: "2" } as Panel;
const info = { channels: [{ id: "2", name: "roles", type: 0 }] } as GuildInfo;
const colors = { enabled: true, mode: "palette", roles: [{ role_id: "1" }] } as RolesView["colors"];

test("automatic and sticky role rows cover active, paused and empty states", () => {
  expect(autoRolesSummary([])).toBe("No auto roles yet");
  expect(autoRolesSummary([{ enabled: true }, { enabled: false }] as AutoRole[])).toBe(
    "2 auto roles · 1 active",
  );
  expect(stickyRolesSummary(false)).toBe("Off");
  expect(stickyRolesSummary(true)).toBe("On");
});

test("panel rows cover empty lists, draft locations and saved locations", () => {
  expect(rolePanelsSummary([])).toBe("No panels yet");
  expect(rolePanelsSummary([panel])).toBe("1 panel");
  expect(rolePanelSummary(panel, info)).toBe("1 role · Buttons · #roles");
  expect(rolePanelSummary({ ...panel, channel_id: null }, info)).toBe("1 role · Buttons · Draft");
});

test("colour roles cover off and empty palettes", () => {
  expect(colorRolesSummary({ ...colors, enabled: false })).toBe("Off");
  expect(colorRolesSummary({ ...colors, roles: [] })).toBe("Palette colours · 0 roles made");
});
