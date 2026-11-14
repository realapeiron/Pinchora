import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../../../lib/api";

export function useAction(path: string) {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: ({
      suffix = "",
      method = "POST",
      body,
    }: {
      suffix?: string;
      method?: "POST" | "PATCH" | "DELETE";
      body?: unknown;
    }) => api(`${path}${suffix}`, method, body),
    onSuccess: () => void cache.invalidateQueries({ queryKey: ["player"] }),
  });
}
