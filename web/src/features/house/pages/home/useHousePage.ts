import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { renderRoom } from "../../../../iso/render";
import { buildScene } from "../../../../iso/scene";
import { useGuilds } from "../../../../lib/queries";
import { t } from "../../../../reef/i18n";
import { loadCharacters, useArt, useHouse, useRoom, type HouseTarget } from "../../api/house";
import { useBotPicture } from "./useBotPicture";

export function useHousePage(target: HouseTarget) {
  const [params, setParams] = useSearchParams();
  const house = useHouse(target);
  const playEnabled = params.get("play") === "1";
  const [mode, setMode] = useState<"view" | "play">("play");
  const playing = playEnabled && mode === "play";
  useEffect(() => {
    if (!playing) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  }, [playing]);
  const art = useArt(!playing);
  const [editMode, setEditing] = useState(false);
  const editing = editMode && house.data?.editable !== false;
  const [phone, setPhone] = useState(() => window.matchMedia("(max-width: 767px)").matches);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 767px)");
    const update = () => {
      setPhone(media.matches);
      if (media.matches) setEditing(false);
    };
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  const guilds = useGuilds();
  const [guild, setGuild] = useState(target.kind === "plaza" ? target.guild : "");
  const selectedGuild = target.kind === "plaza" ? target.guild : guild;
  const [grid, setGrid] = useState(false);
  const [compare, setCompare] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const [visitId, setVisitId] = useState("");

  const first = house.data?.rooms[0]?.idx ?? null;
  const asked = Number(params.get("room"));
  const idx = house.data?.rooms.find((r) => r.idx === asked)?.idx ?? first;
  const room = useRoom(target, idx, !playing);

  // The characters in the room are drawn by the server; the room waits for their sprites.
  const codes = room.data?.characters ?? [];
  const sprites = useQuery({
    queryKey: ["house-characters", ...codes],
    queryFn: async () => {
      if (art.data) await loadCharacters(art.data, codes);
      return true;
    },
    enabled: !playing && art.isSuccess && room.isSuccess,
    staleTime: Infinity,
  });

  const owner = house.data?.owner.name;
  // The bot's picture has no grid, so comparing draws without one.
  const edit = grid && !compare;
  const drawn = useMemo(() => {
    if (playing || editing || !art.data || !room.data || !sprites.isSuccess) return null;
    const scene = buildScene(art.data.content, art.data, room.data, { edit, characterLabel: owner });
    return { scene, rendered: renderRoom(scene) };
  }, [art.data, room.data, sprites.isSuccess, edit, owner, playing, editing]);

  useEffect(() => setHover(null), [drawn]);

  const bot = useBotPicture(target, idx, room.data?.seed ?? null, compare, drawn?.rendered.image ?? null);

  const selectRoom = (nextRoom: number) =>
    setParams(
      (previous) => {
        const next = new URLSearchParams(previous);
        next.set("room", String(nextRoom));
        return next;
      },
      { replace: true },
    );
  const selectScope = (scope: string) => {
    const next = new URLSearchParams(params);
    next.set("scope", scope);
    if (scope === "plaza") next.set("guild", selectedGuild);
    setParams(next);
  };
  const selectGuild = (guildId: string) => {
    setGuild(guildId);
    if (target.kind === "plaza") {
      const next = new URLSearchParams(params);
      next.set("guild", guildId);
      setParams(next);
    }
  };
  const toggleEdit = () => {
    setEditing(!editing);
    setCompare(false);
  };
  const selectMode = (next: "view" | "play") => setMode(next);
  const exitEdit = () => setEditing(false);
  const toggleGrid = () => setGrid(!grid);
  const toggleCompare = () => setCompare(!compare);
  const hoverItem = (item: number | null) => setHover(item);
  const updateVisitId = (id: string) => setVisitId(id);
  const canDecorate = !room.isPending && !!room.data?.fingerprint;
  const title = house.data
    ? house.data.scope === "own"
      ? t("MY HOUSE")
      : t("{name}'S HOUSE", { name: house.data.owner.name.toLocaleUpperCase() })
    : t("HOUSE");
  const summary = house.data?.rooms.find((r) => r.idx === idx);
  const info = drawn && hover !== null ? drawn.scene.items[hover]?.info : null;
  return {
    params,
    selectRoom,
    selectScope,
    selectGuild,
    toggleEdit,
    canDecorate,
    house,
    playEnabled,
    selectMode,
    playing,
    art,
    exitEdit,
    editing,
    phone,
    guilds: guilds.data,
    selectedGuild,
    grid,
    toggleGrid,
    compare,
    toggleCompare,
    hover,
    hoverItem,
    visitId,
    updateVisitId,
    idx,
    room,
    sprites,
    drawn,
    bot,
    title,
    summary,
    info,
  };
}
