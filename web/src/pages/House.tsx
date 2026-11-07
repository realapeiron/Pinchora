import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";

import { Panel } from "../components/Panel";
import { PixelButton } from "../components/PixelButton";
import { RoomStage } from "../components/RoomStage";
import { TopBar } from "../components/TopBar";
import { decodePng } from "../iso/png";
import { renderRoom } from "../iso/render";
import { buildScene } from "../iso/scene";
import type { Me } from "../lib/api";
import { type HouseTarget, loadCharacters, renderUrl, useArt, useHouse, useRoom } from "../lib/house";
import { t } from "../reef/i18n";
import { PageHead } from "./PageHead";
import { Loading, Problem } from "./States";
import "../styles/house.css";

const ReefStage = lazy(() => import("../reef/ReefStage"));

/** `/me/house`: the logged-in player's house. */
export function MyHouse({ me }: { me: Me }) {
  return <House me={me} target={{ kind: "own" }} />;
}

/** `/house/:userId`: another player's house, unless they locked it. */
export function VisitHouse({ me }: { me: Me }) {
  const { userId = "" } = useParams();
  return <House me={me} target={{ kind: "visit", userId }} />;
}

function House({ me, target }: { me: Me; target: HouseTarget }) {
  const navigate = useNavigate();
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
    if (playing || !art.data || !room.data || !sprites.isSuccess) return null;
    const scene = buildScene(art.data.content, art.data, room.data, { edit, characterLabel: owner });
    return { scene, rendered: renderRoom(scene) };
  }, [art.data, room.data, sprites.isSuccess, edit, owner, playing]);

  useEffect(() => setHover(null), [drawn]);

  const bot = useBotPicture(target, idx, room.data?.seed ?? null, compare, drawn?.rendered.image ?? null);

  const title = house.data
    ? house.data.scope === "own"
      ? t("MY HOUSE")
      : t("{name}'S HOUSE", { name: house.data.owner.name.toLocaleUpperCase() })
    : t("HOUSE");
  const summary = house.data?.rooms.find((r) => r.idx === idx);
  const info = drawn && hover !== null ? drawn.scene.items[hover]?.info : null;

  return (
    <div className={playing ? "page house-playing" : "page"}>
      <TopBar me={me} />
      <main className="content content-wide">
        <div className="house-page">
          {!playing && (
            <PageHead
              title={title}
              lead={
                house.data
                  ? t("{tier} · {rooms} rooms · {likes} likes", {
                      tier: house.data.house.tier_name,
                      rooms: house.data.rooms.length,
                      likes: house.data.house.likes,
                    })
                  : target.kind === "own"
                    ? t("Your rooms, as they stand.")
                    : t("Somebody else's rooms, as they stand.")
              }
              action={
                house.data?.scope === "own" ? (
                  <form
                    className="house-visit"
                    onSubmit={(event) => {
                      event.preventDefault();
                      if (/^\d{15,21}$/.test(visitId.trim())) navigate(`/house/${visitId.trim()}`);
                    }}
                  >
                    <input
                      value={visitId}
                      onChange={(event) => setVisitId(event.target.value)}
                      inputMode="numeric"
                      placeholder={t("Player ID to visit")}
                      aria-label={t("Discord ID of a player to visit")}
                    />
                    <PixelButton
                      type="submit"
                      variant="secondary"
                      disabled={!/^\d{15,21}$/.test(visitId.trim())}
                    >
                      {t("Visit")}
                    </PixelButton>
                  </form>
                ) : (
                  <PixelButton variant="secondary" onClick={() => navigate("/me/house")}>
                    {t("My house")}
                  </PixelButton>
                )
              }
            />
          )}
          {house.isPending && <Loading label={t("Opening the door")} />}
          {house.isError && <Problem error={house.error} />}
          {house.isSuccess && (
            <>
              {!playing && house.data.rooms.length > 1 && (
                <div className="house-tabs" role="tablist" aria-label={t("Rooms")}>
                  {house.data.rooms.map((r) => (
                    <PixelButton
                      key={r.idx}
                      role="tab"
                      aria-selected={r.idx === idx}
                      variant={r.idx === idx ? "primary" : "secondary"}
                      onClick={() =>
                        setParams(
                          (previous) => {
                            const next = new URLSearchParams(previous);
                            next.set("room", String(r.idx));
                            return next;
                          },
                          { replace: true },
                        )
                      }
                    >
                      {r.name} <span className="house-tab-count">{r.items}</span>
                    </PixelButton>
                  ))}
                </div>
              )}
              {playEnabled && !playing && (
                <div className="house-options" role="group" aria-label={t("Room mode")}>
                  <PixelButton aria-pressed={playing} onClick={() => setMode("play")}>
                    {t("Play")}
                  </PixelButton>
                  <PixelButton variant="secondary" aria-pressed={!playing} onClick={() => setMode("view")}>
                    {t("View")}
                  </PixelButton>
                </div>
              )}
              {playing && room.data && (
                <Suspense fallback={<Loading label={t("Opening Play")} />}>
                  <ReefStage
                    room={room.data}
                    own={house.data.scope === "own"}
                    roomKey={`house:${house.data.owner.id}:${room.data.idx}`}
                    onLayout={() => void room.refetch()}
                    roomName={summary?.name ?? house.data.house.name}
                    playerName={me.user.global_name ?? me.user.username}
                    onExit={() => setMode("view")}
                    rooms={house.data.rooms}
                    onRoom={(nextRoom) =>
                      setParams(
                        (previous) => {
                          const next = new URLSearchParams(previous);
                          next.set("room", String(nextRoom));
                          return next;
                        },
                        { replace: true },
                      )
                    }
                  />
                </Suspense>
              )}
              {!playing &&
                (room.isPending || art.isPending || (room.isSuccess && sprites.isPending)) &&
                !art.isError && <Loading label={t("Drawing the room")} />}
              {room.isError && <Problem error={room.error} />}
              {!playing && art.isError && <Problem error={art.error} />}
              {drawn && summary && (
                <>
                  <RoomStage
                    rendered={drawn.rendered}
                    hover={hover}
                    onHover={setHover}
                    label={t("{name}, {count} items", { name: summary.name, count: summary.items })}
                    botPicture={compare && bot.url ? bot.url : null}
                  />
                  <div className="house-info" aria-live="polite">
                    {info ? (
                      <>
                        <span className="house-info-name">{info.label}</span>
                        {info.detail && <span className="muted">{info.detail}</span>}
                      </>
                    ) : (
                      <span className="muted">
                        {t("Hover or tap a piece to see what it is. Drag to pan, Ctrl + wheel to zoom.")}
                      </span>
                    )}
                  </div>
                  <div className="house-options">
                    <PixelButton
                      variant={grid ? "primary" : "secondary"}
                      aria-pressed={grid}
                      disabled={compare}
                      onClick={() => setGrid(!grid)}
                    >
                      {t("Tile grid")}
                    </PixelButton>
                    <PixelButton
                      variant={compare ? "primary" : "secondary"}
                      aria-pressed={compare}
                      onClick={() => setCompare(!compare)}
                    >
                      {compare ? t("Back to this view") : t("Compare with Discord")}
                    </PixelButton>
                    {compare && (
                      <span className="muted" role="status">
                        {bot.status}
                      </span>
                    )}
                  </div>
                  <Contents drawn={drawn.scene.items.map((i) => i.info)} />
                </>
              )}
            </>
          )}
        </div>
      </main>
    </div>
  );
}

/** What is in the room, by name, for anyone who cannot point at the picture. */
function Contents({ drawn }: { drawn: { kind: string; label: string }[] }) {
  const counts = new Map<string, number>();
  for (const info of drawn) {
    const key =
      info.kind === "furniture"
        ? info.label
        : `${info.kind === "pet" ? t("Pet") : t("Character")}: ${info.label}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const rows = [...counts].sort(([a], [b]) => a.localeCompare(b));
  return (
    <Panel title={t("In this room")} aside={<span className="pixel-label muted">{drawn.length}</span>}>
      {rows.length === 0 ? (
        <p className="muted">{t("Nothing yet.")}</p>
      ) : (
        <ul className="house-contents">
          {rows.map(([name, n]) => (
            <li key={name}>
              <span>{name}</span>
              <span className="muted">×{n}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

/**
 * The bot's own picture of the room, fetched while comparing, and what it says about this view: the
 * pictures are drawn by different code, so they must agree to the pixel.
 */
function useBotPicture(
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
