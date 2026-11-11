import { expect, test } from "vitest";
import { setLocale } from "../../../i18n";
import type { InviteSettings } from "../api/invites";
import { invitesSummary } from "./invitesSummary";

test("invite rows handle off, empty and configured values", async () => {
  await setLocale("en");
  const settings = { enabled: false, fake_days: 7, temp_enabled: false, temp_max_uses: 3 } as InviteSettings;
  expect(invitesSummary(settings, [])).toEqual({ tracking: "Off", links: "Off", rewards: "No reward roles" });
  expect(
    invitesSummary({ ...settings, enabled: true, temp_enabled: true }, [{ invites: 5, role_id: "1" }]),
  ).toEqual({ tracking: "7 day window", links: "3 uses per link", rewards: "1 reward role" });
});
