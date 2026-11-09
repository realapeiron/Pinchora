// Data for the house pages: the overview and rooms the API answers with, the catalogue and sprite sheet
// the browser draws with, and the character sprites only the server can draw.

import { useQuery } from "@tanstack/react-query";
import roomNames from "../../../../../content/i18n/tr/rooms.json";
import terms from "../../../../../content/i18n/tr/terms.json";
import { locale } from "../../../i18n";

import { Art } from "../../../iso/art";
import { loadAtlas } from "../../../iso/atlas";
import { type Catalog, Content } from "../../../iso/content";
import { decodePng } from "../../../iso/png";
import type { RoomJson } from "../../../iso/scene";
import { api } from "../../../lib/api";

/** Whose house a page shows: the logged-in player's, or another player's. */
export type HouseTarget =
  | { kind: "own" }
  | { kind: "partner" }
  | { kind: "plaza"; guild: string }
  | { kind: "visit"; userId: string };

export interface RoomSummary {
  idx: number;
  room_type: string;
  name: string;
  wallpaper: string;
  floor: string;
  /** How many things stand or hang in the room. */
  items: number;
}

export interface HouseOverview {
  reef_loop?: import("../components/ReefLoop").ReefHome | null;
  editable?: boolean;
  scope: "own" | "partner" | "plaza" | "visit";
  next?: { name: string; price: number; rooms: number; width: number; depth: number } | null;
  owner: { id: string; name: string; you: boolean };
  house: { name: string; tier: number; tier_name: string; likes: number; locked: boolean };
  width: number;
  depth: number;
  rooms: RoomSummary[];
}

type CatalogResponse = Catalog & { assets: { atlas_png: string; atlas_json: string } };

export function housePath(target: HouseTarget, path = "") {
  const root =
    target.kind === "visit" ? `/api/house/visit/${encodeURIComponent(target.userId)}` : "/api/house";
  const query =
    target.kind === "partner"
      ? "?scope=partner"
      : target.kind === "plaza"
        ? `?scope=plaza&guild=${encodeURIComponent(target.guild)}`
        : "";
  return `${root}${path}${query}`;
}
const base = (target: HouseTarget) => housePath(target);

export function useHouse(target: HouseTarget) {
  return useQuery({
    queryKey: [base(target)],
    queryFn: () => api<HouseOverview>(base(target)),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
    retry: false,
  });
}

/** One room. Visible tabs refresh every ten seconds; edits still use the last confirmed fingerprint. */
export function useRoom(target: HouseTarget, idx: number | null, refresh = true) {
  return useQuery({
    queryKey: [housePath(target, `/rooms/${idx}`)],
    queryFn: () => api<RoomJson>(housePath(target, `/rooms/${idx}`)),
    enabled: idx !== null,
    refetchInterval: refresh ? 10_000 : false,
    refetchIntervalInBackground: false,
    retry: false,
  });
}

/** The URL of the bot's own picture of a room, for comparing. */
export function renderUrl(target: HouseTarget, idx: number, seed: number, scale = 2): string {
  const path = housePath(target, `/rooms/${idx}/render.png`);
  return `${path}${path.includes("?") ? "&" : "?"}scale=${scale}&seed=${seed}`;
}

/** The catalogue and the atlas, read once: the art every room is drawn from. */
export function useArt(enabled = true) {
  return useQuery({
    queryKey: ["house-art"],
    enabled,
    queryFn: async () => {
      const catalog = await api<CatalogResponse>("/api/house/content");
      const atlas = await loadAtlas(catalog.assets.atlas_png, catalog.assets.atlas_json);
      return new Art(atlas, new Content(catalog));
    },
    staleTime: Infinity,
    gcTime: Infinity,
    retry: false,
  });
}

/** Fetches the sprites of the characters a room lists that `art` does not have yet. */
export async function loadCharacters(art: Art, codes: string[]): Promise<void> {
  await Promise.all(
    codes
      .filter((code) => !art.hasCharacter(code))
      .map(async (code) => {
        try {
          const response = await fetch(`/api/house/character.png?code=${encodeURIComponent(code)}`, {
            credentials: "same-origin",
          });
          if (response.ok)
            art.setCharacter(code, await decodePng(new Uint8Array(await response.arrayBuffer())));
        } catch {
          // The room still draws; the figure keeps its tile and stays invisible.
        }
      }),
  );
}

/** Use the same Turkish content names as Discord; user-authored house names stay untouched. */
export function roomLabel(id: string, fallback: string) {
  return locale() === "tr"
    ? ((roomNames as Record<string, { name: string }>)[id]?.name ?? fallback)
    : fallback;
}
export function tierLabel(tier: number, fallback: string) {
  return locale() === "tr"
    ? ((terms as Record<string, { name: string }>)[`tier.${tier}`]?.name ?? fallback)
    : fallback;
}
