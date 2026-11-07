import { expect, it, vi } from "vitest";
import { AssetLoader, ByteCache, ByteLedger } from "./loader";
import type { AssetManifest } from "./manifest";
import { fixtureBytes } from "../iso/testing";
it("evicts LRU by decoded bytes, replaces, disposes and rejects oversize retention", () => {
  const dispose = vi.fn();
  const c = new ByteCache<string>(8, dispose);
  c.set("a", "A", 4);
  c.set("b", "B", 4);
  c.get("a");
  c.set("c", "C", 4);
  expect(c.get("b")).toBeUndefined();
  expect(dispose).toHaveBeenCalledWith("B");
  c.set("a", "new", 3);
  expect(c.bytes).toBe(7);
  c.set("big", "big", 9);
  expect(c.bytes).toBe(7);
  c.clear();
  expect(c.bytes).toBe(0);
});
it("deduplicates in-flight pages and counts response bytes only once", async () => {
  const bytes = fixtureBytes("empty_8x8.png");
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(new Uint8Array(bytes)));
  try {
    const manifest: AssetManifest = {
      version: 1,
      pages: { test: { url: "/test.png", bytes: bytes.length, width: 540, height: 399 } },
      sprites: {},
      looks: {},
    };
    const loader = new AssetLoader(manifest);
    const [a, b] = await Promise.all([loader.page("test"), loader.page("test")]);
    expect(a).toBe(b);
    await loader.page("test");
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(loader.ledger.bytes).toBe(bytes.length);
    expect(loader.cache.bytes).toBe(540 * 399 * 4);
  } finally {
    fetcher.mockRestore();
  }
});
it("retries failed requests and does not poison the cache", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("no", { status: 500 }));
  try {
    const loader = new AssetLoader({
      version: 1,
      pages: { test: { url: "/test", bytes: 1, width: 1, height: 1 } },
      sprites: {},
      looks: {},
    });
    await expect(loader.page("test")).rejects.toThrow("500");
    await expect(loader.page("test")).rejects.toThrow("500");
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(loader.cache.bytes).toBe(0);
  } finally {
    fetcher.mockRestore();
  }
});
it("freezes first-interaction accounting", () => {
  const l = new ByteLedger();
  l.bytes = 123;
  l.markPlayable();
  l.bytes = 345;
  l.markPlayable();
  expect(l.firstInteraction).toBe(123);
});
