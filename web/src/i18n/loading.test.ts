import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const english = { en: { Hmm: "Hmm", Loading: "Loading", fallback: "English fallback" } };
const turkish = { tr: { Hmm: "Bir sorun var", Loading: "Yükleniyor" } };

beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal("localStorage", { getItem: () => "en", setItem: vi.fn(), removeItem: vi.fn() });
  vi.doMock("./en", () => english);
  vi.doMock("./tr", () => turkish);
});
afterEach(() => {
  vi.doUnmock("./en");
  vi.doUnmock("./tr");
  vi.unstubAllGlobals();
  vi.resetModules();
});

describe("locale loading", () => {
  it("shares in-flight loads and allows a failed catalogue to be requested again", async () => {
    const pending = deferred<typeof turkish>();
    vi.doMock("./tr", () => pending.promise);
    const i18n = await import("./index");
    const first = i18n.loadLocale("tr");
    expect(i18n.loadLocale("tr")).toBe(first);
    pending.reject(new Error("chunk unavailable"));
    await expect(first).rejects.toThrow();
    vi.doMock("./tr", () => turkish);
    await i18n.ready();
    expect(i18n.translate("Hmm", {}, "tr")).toBe("Bir sorun var");
  });

  it("keeps the latest choice when two changes race a catalogue download", async () => {
    const pending = deferred<typeof turkish>();
    vi.doMock("./tr", () => pending.promise);
    const i18n = await import("./index");
    await i18n.loadLocale("en");
    const older = i18n.setLocale("tr");
    const newer = i18n.setLocale("en");
    expect(i18n.locale()).toBe("en");
    expect(i18n.t("Hmm")).toBe("Hmm");
    pending.resolve(turkish);
    await Promise.all([older, newer]);
    expect(i18n.locale()).toBe("en");
    expect(localStorage.setItem).toHaveBeenCalledExactlyOnceWith("pinchora.language.visitor", "en");
  });

  it("loads reverse API lookup and English fallback even when the first choice is Turkish", async () => {
    vi.stubGlobal("localStorage", { getItem: () => "tr", setItem: vi.fn() });
    const i18n = await import("./index");
    await i18n.setLocale("tr");
    expect(i18n.hasMessage("Hmm")).toBe(true);
    expect(i18n.translate("fallback")).toBe("English fallback");
    expect(i18n.serverText("title", { title: "Hmm" })).toBe("Bir sorun var");
    await i18n.setLocale("en");
    expect(i18n.serverText("title", { title: "Bir sorun var" })).toBe("Hmm");
    const reef = await import("../reef/i18n");
    expect(reef.t("Hmm")).toBe("Hmm");
  });

  it("ignores an unsupported stored language", async () => {
    vi.stubGlobal("localStorage", { getItem: () => "fr" });
    vi.stubGlobal("navigator", { language: "tr-TR" });
    const i18n = await import("./index");
    await i18n.ready();
    expect(i18n.locale()).toBe("tr");
    expect(i18n.t("Loading")).toBe("Yükleniyor");
  });

  it("does not let a failed older save roll back a newer choice, and saves in order", async () => {
    const i18n = await import("./index");
    await i18n.ready();
    const pending = deferred<void>();
    const started = deferred<void>();
    const saveFirst = vi.fn(() => {
      started.resolve();
      return pending.promise;
    });
    const saveLast = vi.fn(async () => undefined);
    const older = i18n.setLocale("tr", saveFirst);
    await started.promise;
    const newer = i18n.setLocale("en", saveLast);
    pending.reject(new Error("save failed"));
    await Promise.all([older, newer]);
    expect(i18n.locale()).toBe("en");
    expect(saveLast).toHaveBeenCalledExactlyOnceWith("en");
  });

  it("restores the prior choice if the current save fails", async () => {
    const i18n = await import("./index");
    await expect(
      i18n.setLocale("tr", async () => {
        throw new Error("save failed");
      }),
    ).rejects.toThrow("save failed");
    expect(i18n.locale()).toBe("en");
    expect(localStorage.setItem).toHaveBeenLastCalledWith("pinchora.language.visitor", "en");
  });
});
