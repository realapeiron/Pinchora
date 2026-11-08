import type { QueryClient } from "@tanstack/react-query";
/** Rewards use the existing player, inventory, pet and house caches on both transports. */
export const economyKey = (key: readonly unknown[]) => {
  const name = String(key[0]);
  return (
    ["player", "economy"].includes(name) ||
    name.startsWith("/api/me") ||
    name.startsWith("/api/house") ||
    name.includes("inventory") ||
    name.includes("pet")
  );
};
export const refreshEconomy = (client: QueryClient) =>
  client.invalidateQueries({ predicate: (q) => economyKey(q.queryKey) });
