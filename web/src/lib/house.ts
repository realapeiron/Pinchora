// Data for the house pages: the overview and rooms the API answers with, the catalogue and sprite sheet
// the browser draws with, and the character sprites only the server can draw.

import { useQuery } from "@tanstack/react-query";

import { Art } from "../iso/art";
import { loadAtlas } from "../iso/atlas";
import { type Catalog, Content } from "../iso/content";
import { decodePng } from "../iso/png";
import type { RoomJson } from "../iso/scene";
import { api } from "./api";

/** Whose house a page shows: the logged-in player's, or another player's. */
export type HouseTarget = { kind: "own" } | { kind: "visit"; userId: string };

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
  scope: "own" | "visit";
  owner: { id: string; name: string; you: boolean };
  house: { name: string; tier: number; tier_name: string; likes: number; locked: boolean };
  width: number;
  depth: number;
  rooms: RoomSummary[];
}

type CatalogResponse = Catalog & { assets: { atlas_png: string; atlas_json: string } };

const base = (target: HouseTarget) =>
  target.kind === "own" ? "/api/house" : `/api/house/visit/${encodeURIComponent(target.userId)}`;

export function useHouse(target: HouseTarget) {
  return useQuery({
    queryKey: [base(target)],
    queryFn: () => api<HouseOverview>(base(target)),
    retry: false,
  });
}

/** One room. Refetched every minute so the pets move on with the ten-minute window the server picks them by. */
export function useRoom(target: HouseTarget, idx: number | null) {
  return useQuery({
    queryKey: [`${base(target)}/rooms/${idx}`],
    queryFn: () => api<RoomJson>(`${base(target)}/rooms/${idx}`),
    enabled: idx !== null,
    refetchInterval: 60_000,
    retry: false,
  });
}

/** The URL of the bot's own picture of a room, for comparing. */
export function renderUrl(target: HouseTarget, idx: number, seed: number, scale = 2): string {
  return `${base(target)}/rooms/${idx}/render.png?scale=${scale}&seed=${seed}`;
}

/** The catalogue and the atlas, read once: the art every room is drawn from. */
export function useArt() {
  return useQuery({
    queryKey: ["house-art"],
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
