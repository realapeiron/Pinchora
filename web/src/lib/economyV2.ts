import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { useLocale } from "../i18n";
export interface Offer {
  shells: number;
  items: { item_id: string; quantity: number }[];
}
export interface EconomyPlayer {
  job: string;
  shifts: number;
  prestige: number;
  chests: number;
  provision_at: string | null;
  provision_claimed: number;
  shift_token: number;
  shift_step: number;
  shift_started: string | null;
  season: number;
  season_xp: number;
}
export interface Auction {
  id: number;
  seller_id: string;
  item_id: string;
  start_price: number;
  bid: number;
  bidder_id: string | null;
  ends_at: string;
  hard_end: string;
  status: string;
  minimum_bid: number;
}
export interface Trade {
  id: number;
  user_a: string;
  user_b: string;
  offer_a: Offer;
  offer_b: Offer;
  confirmed_a: boolean;
  confirmed_b: boolean;
  expires_at: string;
  status: string;
}
export interface Economy {
  user_id: string;
  player: EconomyPlayer | null;
  shifts_today: number;
  next_shift: string | null;
  season: number;
  season_ends: string;
  season_claimed: number;
  trades: Trade[];
  auctions: Auction[];
  inventory: { id: string; name: string; quantity: number }[];
  odds: [number, number][];
}
export interface Settings {
  pool_goal: number;
  pool_total: number;
  shop_min_level: number;
  shop_min_days: number;
  shop_enabled: boolean;
  revision: number;
}
export interface PointsPreview {
  count: number;
  total: string;
  version: string;
}
export function settingsDraft(s: Settings) {
  return {
    pool_goal: s.pool_goal,
    shop_min_level: s.shop_min_level,
    shop_min_days: s.shop_min_days,
    shop_enabled: s.shop_enabled,
  };
}
export function useEconomy() {
  const locale = useLocale();
  return useQuery({
    queryKey: ["economy", locale],
    queryFn: () => api<Economy>(`/api/me/economy?lang=${locale}`),
    refetchInterval: 15000,
  });
}
export function useEconomyAction() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (command: Record<string, unknown>) =>
      api<Record<string, unknown>>("/api/me/economy/action", "POST", {
        request_id: crypto.randomUUID(),
        command,
      }),
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["economy"] }),
        client.invalidateQueries({ queryKey: ["player"] }),
      ]);
    },
  });
}
export function provisionReady(p: EconomyPlayer | null, now: number) {
  return p?.provision_at
    ? Math.max(
        0,
        Math.min(120, Math.floor((now - Date.parse(p.provision_at)) / 3600000) * 10) - p.provision_claimed,
      )
    : 0;
}
export function seasonProgress(data: Economy) {
  return data.player?.season === data.season ? data.player.season_xp : 0;
}
