import { expect, test } from "@playwright/test";

test("AutoMod API saves profiles and rules, previews without hits, and enforces Manage Server", async ({
  request,
}) => {
  const run = Date.now();
  const guild = `${run}3`;
  const login = await request.post("/auth/dev-login", {
    data: {
      user_id: `${run}4`,
      username: "automod_manager",
      guilds: [{ id: guild, name: "AutoMod API", owner: true, permissions: "0", icon: null }],
    },
  });
  expect(login.ok()).toBeTruthy();
  const headers = { "X-CSRF-Token": (await login.json()).csrf as string };
  const path = `/api/guilds/${guild}/automod`;
  const profile = await request.post(`${path}/profile`, { headers, data: { profile: "standard" } });
  expect(profile.ok(), await profile.text()).toBeTruthy();
  const data = await profile.json();
  expect(data.config.rules).toHaveLength(16);
  expect(Date.parse(data.config.observe_until) - Date.now()).toBeGreaterThan(47 * 3600_000);
  const caps = data.config.rules.find((r: { kind: string }) => r.kind === "caps");
  caps.params.threshold = 90;
  caps.log_only = true;
  const saved = await request.put(path, { headers, data: data.config });
  expect(saved.ok(), await saved.text()).toBeTruthy();
  const config = (await saved.json()).config;
  expect(config.rules.find((r: { kind: string }) => r.kind === "caps").params.threshold).toBe(90);
  const preview = await request.post(`${path}/test`, {
    headers,
    data: { text: "THIS MESSAGE IS TOO LOUD", config },
  });
  expect(preview.ok()).toBeTruthy();
  expect((await preview.json()).note).toContain("Preview only");
  expect((await (await request.get(path)).json()).hits).toEqual([]);
  expect((await request.delete(`${path}/strikes/${run}5`, { headers })).ok()).toBeTruthy();
  const off = await request.post(`${path}/profile`, { headers, data: { profile: "off" } });
  const paused = (await off.json()).config;
  expect(paused.profile).toBe("off");
  expect(paused.rules).toEqual(config.rules);
  await request.post("/auth/dev-login", {
    data: {
      user_id: `${run}6`,
      username: "automod_member",
      guilds: [{ id: guild, name: "AutoMod API", owner: false, permissions: "0", icon: null }],
    },
  });
  expect((await request.get(path)).status()).toBe(403);
});
