import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../../../../../lib/api";

export function useCollectionAction() {
  const cache = useQueryClient();
  return useMutation({
    mutationFn: ({
      path,
      method,
      body,
    }: {
      path: string;
      method: "POST" | "PUT" | "DELETE";
      body?: unknown;
    }) => api(`/api/me/collection/${path}`, method, body),
    onSuccess: () => void cache.invalidateQueries({ queryKey: ["player"] }),
  });
}
