import { describe, expect, it } from "vitest";
import {
  petRename,
  changeCharacterColor,
  characterKey,
  outfitChanged,
  profileChanges,
  playerPreviewResponse,
  reorderBadge,
  toggleBadge,
  wearAccessory,
  type ProfileDraft,
  type Accessory,
} from "./webPlayer";

const profile: ProfileDraft = {
  bio: "",
  background: null,
  badges: [],
  badges_set: false,
  title: null,
  accent: null,
  panel_opacity: 1,
  frame: "plain",
  showcase_fish: null,
  showcase_pet: null,
  showcase_item: null,
  showcase_house: false,
};
describe("player studio drafts", () => {
  it("keeps badge order, refuses a seventh, and allows removing from a full card", () => {
    const ids = ["a", "b", "c", "d", "e", "f"];
    expect(toggleBadge(ids, "g")).toEqual(ids);
    expect(toggleBadge(ids, "c")).toEqual(["a", "b", "d", "e", "f"]);
    expect(reorderBadge(ids, 5, 0)).toEqual(["f", "a", "b", "c", "d", "e"]);
    expect(reorderBadge(ids, 0, -1)).toBe(ids);
    expect(ids).toEqual(["a", "b", "c", "d", "e", "f"]);
  });
  it("distinguishes automatic badges from a deliberately empty card and tracks only real changes", () => {
    expect(profileChanges(profile, { ...profile })).toBe(0);
    expect(profileChanges(profile, { ...profile, badges_set: true })).toBe(1);
    expect(profileChanges({ ...profile, badges: ["a", "b"] }, { ...profile, badges: ["b", "a"] })).toBe(1);
  });
  it("preserves accessories and other channels when editing colours", () => {
    expect(changeCharacterColor("tidewalker.1.2.3.4.5.6+hat*0", "eyes", "#112233")).toBe(
      "tidewalker.1.2.3.4.#112233.6+hat*0",
    );
  });
  it("replaces only the matching accessory slot and can retain the zero tint", () => {
    const catalog = [
      { id: "cap", slot: "head" },
      { id: "hat", slot: "head" },
      { id: "bag", slot: "back" },
    ] as Accessory[];
    expect(wearAccessory("tidewalker.1.1.0.2.0.0+cap+bag", "head", "hat", catalog, "0")).toBe(
      "tidewalker.1.1.0.2.0.0+bag+hat*0",
    );
    expect(wearAccessory("tidewalker.1.1.0.2.0.0+cap+bag", "head", null, catalog)).toBe(
      "tidewalker.1.1.0.2.0.0+bag",
    );
  });
});

it("ignores cosmetic encoding differences and trimmed outfit names", () => {
  expect(characterKey("tidewalker.1.2.3.4.5.6+hat+bag")).toBe(characterKey("tidewalker.1.2.3.4.5.6+bag+hat"));
  expect(profileChanges({ ...profile, accent: "#ABCDEF" }, { ...profile, accent: "#abcdef" })).toBe(0);
  for (const accent of ["#abc", "ABC", " aabbcc ", "##abc"]) {
    expect(profileChanges({ ...profile, accent: "#aabbcc" }, { ...profile, accent })).toBe(0);
  }
  const saved = { spec: "tidewalker.1.2.3.4.5.6+hat+bag", name: "Reef" };
  expect(outfitChanged(saved, saved.spec, "  Reef  ")).toBe(false);
  expect(outfitChanged(saved, saved.spec, "")).toBe(false);
  expect(outfitChanged(saved, saved.spec, " ")).toBe(false);
  expect(outfitChanged(saved, saved.spec, "x".repeat(33))).toBe(false);
  expect(outfitChanged(saved, saved.spec, "New reef")).toBe(true);
  expect(outfitChanged(undefined, saved.spec, "Reef")).toBe(true);
});

it("shows a friendly error for empty or non-JSON preview refusals", async () => {
  for (const response of [new Response(null, { status: 414 }), new Response("busy", { status: 503 })]) {
    await expect(playerPreviewResponse(response)).rejects.toThrow(
      "Preview unavailable. Check your choices and try again.",
    );
  }
  await expect(
    playerPreviewResponse(new Response(JSON.stringify({ error: { message: "Locked" } }), { status: 422 })),
  ).rejects.toThrow("Locked");
});

it("enables pet renames only for normalized changes and sends only the writable field", () => {
  expect(petRename("Sea Pip", undefined)).toBeNull();
  expect(petRename("Sea Pip", "  Sea   Pip  ")).toBeNull();
  expect(petRename("Pip", " ")).toBeNull();
  expect(petRename("Pip", "x".repeat(21))).toBeNull();
  expect(petRename("Pip", "🐚".repeat(20))).toEqual({ name: "🐚".repeat(20) });
  expect(petRename("Pip", "  Sea   Pip  ")).toEqual({ name: "Sea Pip" });
});
