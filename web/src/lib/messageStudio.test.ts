import { expect, it, vi } from "vitest";
import { componentRows, importKit, type Kit } from "./messageStudio";

const kit: Kit = {
  name: "Example",
  content: "Hi",
  variants: [],
  embeds: [],
  components: [],
  allow_mass_mentions: true,
};

it("requires server validation before accepting untrusted imports and clears ping approval", async () => {
  const validate = vi.fn().mockResolvedValue(kit);
  expect(await importKit(JSON.stringify(kit), validate)).toEqual({ ...kit, allow_mass_mentions: false });
  expect(validate).toHaveBeenCalledWith(kit);
  const reject = vi.fn().mockRejectedValue(new Error("invalid schema"));
  await expect(importKit('{"components":[null],"version":99}', reject)).rejects.toThrow("invalid schema");
});

it("rejects invalid and oversized JSON before any request, counting UTF-8 bytes", async () => {
  const validate = vi.fn();
  await expect(importKit("not JSON", validate)).rejects.toThrow();
  await expect(importKit(JSON.stringify({ content: "🦀".repeat(9000) }), validate)).rejects.toThrow(
    "oversized",
  );
  expect(validate).not.toHaveBeenCalled();
});

it("uses the same five-button packing as the server, with selects on separate rows", () => {
  const button = { type: "button" as const };
  const select = { type: "select" as const };
  expect(componentRows(Array(25).fill(button)).map((row) => row.length)).toEqual([5, 5, 5, 5, 5]);
  expect(componentRows([button, button, select, button, select]).map((row) => row.length)).toEqual([
    2, 1, 1, 1,
  ]);
});

it("localizes server validation errors and hides untranslated fallback details", async () => {
  const { studioError } = await import("./messageStudio");
  const { ApiError } = await import("./api");
  const { setLocale } = await import("../i18n");
  await setLocale("tr");
  try {
    expect(
      studioError(
        new ApiError(422, "invalid", "Only a server administrator can allow everyone and role mentions."),
        "fallback",
      ),
    ).toContain("sunucu yöneticisi");
    expect(studioError(new ApiError(422, "invalid", "Internal English detail"), "fallback")).toContain(
      "Discord sınırlarını",
    );
  } finally {
    await setLocale("en");
  }
});
