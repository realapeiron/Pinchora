import { useQuery } from "@tanstack/react-query";

import { api } from "../lib/api";

/** What `GET /api/public/landing` answers: the totals (cached on the server) and who is looking. */
export interface Landing {
  stats: { servers: number; players: number; houses: number; pets: number } | null;
  viewer: { name: string; avatar_url: string } | null;
}

export function useLanding() {
  return useQuery({
    queryKey: ["/api/public/landing"],
    queryFn: () => api<Landing>("/api/public/landing"),
    staleTime: 5 * 60_000,
    retry: false,
  });
}
