import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import { decodePng } from "../../../../iso/png";
import { t } from "../../../../reef/i18n";
import { renderUrl, type HouseTarget } from "../../api/house";

/**
 * The bot's own picture of the room, fetched while comparing, and what it says about this view: the
 * pictures are drawn by different code, so they must agree to the pixel.
 */
export function useBotPicture(
  target: HouseTarget,
  idx: number | null,
  seed: number | null,
  on: boolean,
  mine: { width: number; height: number; data: Uint8ClampedArray } | null,
) {
  const query = useQuery({
    queryKey: ["house-bot-picture", target, idx, seed],
    queryFn: async () => {
      const response = await fetch(renderUrl(target, idx ?? 0, seed ?? 0, 1), {
        credentials: "same-origin",
      });
      if (!response.ok) throw new Error(t("The server answered {status}.", { status: response.status }));
      const bytes = new Uint8Array(await response.arrayBuffer());
      return { bytes, image: await decodePng(bytes) };
    },
    enabled: on && idx !== null && seed !== null,
    staleTime: Infinity,
  });
  const url = useMemo(
    () => (query.data ? URL.createObjectURL(new Blob([query.data.bytes], { type: "image/png" })) : null),
    [query.data],
  );
  useEffect(() => () => (url ? URL.revokeObjectURL(url) : undefined), [url]);
  let status = t("Fetching Discord's picture…");
  if (query.isError) status = t("Could not fetch Discord's picture.");
  else if (query.data && mine) {
    const theirs = query.data.image;
    if (theirs.width !== mine.width || theirs.height !== mine.height) {
      status = t("Discord’s picture is {theirs}, this view {mine}.", {
        theirs: `${theirs.width}×${theirs.height}`,
        mine: `${mine.width}×${mine.height}`,
      });
    } else {
      let differing = 0;
      for (let i = 0; i < theirs.data.length; i += 4) {
        if (
          theirs.data[i] !== mine.data[i] ||
          theirs.data[i + 1] !== mine.data[i + 1] ||
          theirs.data[i + 2] !== mine.data[i + 2]
        ) {
          differing++;
        }
      }
      status =
        differing === 0
          ? t("Showing Discord's picture: identical to this view.")
          : t("Discord’s picture: {count} pixels differ from this view.", { count: differing });
    }
  }
  return { url, status };
}
