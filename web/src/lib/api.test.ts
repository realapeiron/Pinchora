import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiError, api, needsLogin, setCsrfToken } from "./api";

function reply(status: number, body?: unknown) {
  return vi.fn(
    async (_url: string, _init: RequestInit) =>
      new Response(body === undefined ? null : JSON.stringify(body), { status }),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  setCsrfToken(null);
});

describe("api", () => {
  it("sends reads without a body or token", async () => {
    const fetch = reply(200, { ok: true });
    vi.stubGlobal("fetch", fetch);
    setCsrfToken("t0ken");
    expect(await api("/api/me")).toEqual({ ok: true });
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe("/api/me");
    expect(init.method).toBe("GET");
    expect(init.body).toBeUndefined();
    expect((init.headers as Record<string, string>)["X-CSRF-Token"]).toBeUndefined();
  });

  it("sends writes as JSON with the CSRF token", async () => {
    const fetch = reply(200, { leveling_enabled: false });
    vi.stubGlobal("fetch", fetch);
    setCsrfToken("t0ken");
    await api("/api/guilds/1/settings", "PATCH", { leveling_enabled: false });
    const init = fetch.mock.calls[0]![1];
    const headers = init.headers as Record<string, string>;
    expect(headers["X-CSRF-Token"]).toBe("t0ken");
    expect(headers["Content-Type"]).toBe("application/json");
    expect(init.body).toBe('{"leveling_enabled":false}');
    expect(init.credentials).toBe("same-origin");
  });

  it("turns error envelopes into ApiError", async () => {
    vi.stubGlobal(
      "fetch",
      reply(403, { error: { code: "bot_missing", message: "Not here.", invite_url: "https://x/invite" } }),
    );
    const error = await api("/api/guilds/1").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({ status: 403, code: "bot_missing", inviteUrl: "https://x/invite" });
    expect(needsLogin(error)).toBe(false);
  });

  it("copes with empty and non-JSON answers", async () => {
    vi.stubGlobal("fetch", reply(204));
    expect(await api("/auth/logout", "POST")).toBeUndefined();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response("<html>", { status: 502 })),
    );
    const error = await api("/api/me").catch((e: unknown) => e);
    expect(error).toMatchObject({ status: 502, code: "http_502" });
    vi.stubGlobal("fetch", reply(401, { error: { code: "unauthorized", message: "Log in." } }));
    expect(needsLogin(await api("/api/me").catch((e: unknown) => e))).toBe(true);
  });
});
