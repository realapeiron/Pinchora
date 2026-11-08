import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "./api";
import { useLocale, t } from "../i18n";

export interface Choice {
  id: string;
  name: string;
}
export interface Badge extends Choice {
  description: string;
  earned: boolean;
  new: boolean;
  progress: number | null;
  target: number | null;
  unit: string | null;
  price: number | null;
}
export interface Background extends Choice {
  owned: boolean;
  price: number;
}
export interface ProfileDraft {
  bio: string;
  background: string | null;
  badges: string[];
  badges_set: boolean;
  title: string | null;
  accent: string | null;
  panel_opacity: number;
  frame: string;
  showcase_fish: string | null;
  showcase_pet: string | null;
  showcase_item: string | null;
  showcase_house: boolean;
}
export interface ProfileData {
  profile: ProfileDraft;
  shells: number;
  level: number;
  badges: Badge[];
  backgrounds: Background[];
  titles: (Choice & { description: string; owned: boolean })[];
  frames: (Choice & { level: number })[];
  opacities: { id: number; name: string }[];
  accents: { name: string; value: string }[];
  showcase: { fish: Choice[]; pets: Choice[]; items: Choice[]; house: boolean };
}
export interface HomeData {
  shells: number;
  pearls: number;
  level: number;
  xp_current: number;
  xp_needed: number;
  streak: number;
  pets: (Choice & { species: string })[];
  house: { name: string; room: number } | null;
  discord_url: string;
  birthday: { month: number; day: number; year: number | null; tz: string; public: boolean } | null;
  roblox: { name: string; id: string; method: string } | null;
}
export interface WalletData {
  shells: number;
  pearls: number;
  ledger: { delta: number; reason: string; created_at: string }[];
}
export interface Accessory extends Choice {
  slot: string;
  tintable: boolean;
  price: number;
  owned: boolean;
  buyable: boolean;
  unlock: string;
}
export interface CharacterData {
  spec: string;
  shells: number;
  catalog: {
    looks: Choice[];
    channels: (Choice & { swatches: { id: number; value: string }[] })[];
    tints: { id: number; value: string }[];
    accessories: Accessory[];
    presets: (Choice & { spec: string })[];
  };
  outfits: { slot: number; name: string; spec: string }[];
}
export function usePlayer<T>(path: string) {
  const language = useLocale();
  return useQuery({
    queryKey: ["player", path, language],
    queryFn: () => api<T>(`/api/me/${path}${path.includes("?") ? "&" : "?"}lang=${language}`),
    placeholderData: (previous) => previous,
  });
}
/** Keep the editor mounted even if an uncached language request fails after placeholder data expires. */
export function usePlayerStudio<T>(path: string) {
  const query = usePlayer<T>(path);
  const [last, setLast] = useState<T>();
  useEffect(() => {
    if (query.data !== undefined) setLast(query.data);
  }, [query.data]);
  return { ...query, data: query.data ?? last };
}
export function reorderBadge(ids: string[], from: number, to: number): string[] {
  if (from < 0 || from >= ids.length || to < 0 || to >= ids.length || from === to) return ids;
  const next = [...ids];
  const [id] = next.splice(from, 1);
  next.splice(to, 0, id!);
  return next;
}
export function toggleBadge(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids.filter((v) => v !== id) : ids.length < 6 ? [...ids, id] : ids;
}
function accentKey(value: string | null): string | null {
  if (value === null) return null;
  const hex = value.trim().replace(/^#+/, "").toLowerCase();
  if (/^[a-f0-9]{3}$/.test(hex)) return `#${[...hex].map((c) => c + c).join("")}`;
  return /^[a-f0-9]{6}$/.test(hex) ? `#${hex}` : value;
}
export function characterKey(code: string): string {
  const [base = "", ...accessories] = code.toLowerCase().split("+");
  // The server orders accessories by slot; changing the editing order is not a new look.
  return [base, ...accessories.sort()].join("+");
}
export function outfitChanged(
  saved: { spec: string; name: string } | undefined,
  spec: string,
  name: string,
): boolean {
  const trimmed = name.trim();
  return (
    [...trimmed].length > 0 &&
    [...trimmed].length <= 32 &&
    (!saved || characterKey(saved.spec) !== characterKey(spec) || saved.name !== trimmed)
  );
}
export function profileChanges(a: ProfileDraft, b: ProfileDraft): number {
  return (Object.keys(a) as (keyof ProfileDraft)[]).filter((k) =>
    k === "accent"
      ? accentKey(a.accent) !== accentKey(b.accent)
      : JSON.stringify(a[k]) !== JSON.stringify(b[k]),
  ).length;
}
export async function playerPreviewResponse(response: Response): Promise<Blob> {
  if (!response.ok) {
    // URI limits and upstream failures may have an empty or non-JSON response body.
    const data = await response.json().catch(() => null);
    throw new Error(data?.error?.message ?? t("Preview unavailable. Check your choices and try again."));
  }
  return response.blob();
}
export function usePlayerPreview(path: string) {
  const language = useLocale();
  const [debounced, setDebounced] = useState(path);
  const [src, setSrc] = useState<string>();
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(path), 350);
    return () => window.clearTimeout(timer);
  }, [path]);
  const query = useQuery({
    queryKey: ["player-preview", debounced, language],
    queryFn: async ({ signal }) => {
      const response = await fetch(`${debounced}${debounced.includes("?") ? "&" : "?"}lang=${language}`, {
        credentials: "same-origin",
        signal,
      });
      return playerPreviewResponse(response);
    },
    retry: false,
    staleTime: 0,
    gcTime: 1000,
  });
  useEffect(() => {
    if (!query.data) return;
    const url = URL.createObjectURL(query.data);
    setSrc(url);
    return () => URL.revokeObjectURL(url);
  }, [query.data]);
  return {
    src,
    error: query.error,
    pending: query.isFetching || path !== debounced,
    retry: () => query.refetch(),
  };
}
export function changeCharacterColor(code: string, channel: string, value: string): string {
  const [base = "", ...accessories] = code.split("+");
  const parts = base.split(".");
  const index = ["look", "skin", "hair", "top", "bottom", "eyes", "shoes"].indexOf(channel);
  if (index < 0) return code;
  parts[index] = value;
  return [parts.join("."), ...accessories].join("+");
}
export function wearAccessory(
  code: string,
  slot: string,
  id: string | null,
  catalog: Accessory[],
  tint?: string,
): string {
  const [base = "", ...worn] = code.split("+");
  const next = worn.filter((v) => catalog.find((a) => a.id === v.split("*")[0])?.slot !== slot);
  if (id) next.push(id + (tint ? `*${tint}` : ""));
  return [base, ...next].join("+");
}
