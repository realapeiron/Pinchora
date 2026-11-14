import { lazy, Suspense } from "react";
import { useParams, useSearchParams } from "react-router";
import { PixelButton } from "../../../components/PixelButton";
import { TopBar } from "../../../components/TopBar";
import { Loading, Problem } from "../../../components/States";
import type { Me } from "../../../lib/api";
import { housePath, roomLabel, type HouseTarget } from "../api/house";
import { t } from "../../../reef/i18n";
import { Contents } from "./home/Contents";
import { Header } from "./home/Header";
import { Navigation } from "./home/Navigation";
import { useHousePage } from "./home/useHousePage";

const ReefLoop = lazy(() => import("../components/ReefLoop").then((m) => ({ default: m.ReefLoop })));
const RoomStage = lazy(() => import("../components/RoomStage").then((m) => ({ default: m.RoomStage })));
const HouseEditor = lazy(() => import("../components/HouseEditor").then((m) => ({ default: m.HouseEditor })));

const ReefStage = lazy(() => import("../../../reef/ReefStage"));

/** `/me/house`: the logged-in player's house. */
export function MyHouse({ me }: { me: Me }) {
  const [params] = useSearchParams();
  const scope = params.get("scope");
  const target: HouseTarget =
    scope === "partner"
      ? { kind: "partner" }
      : scope === "plaza" && params.get("guild")
        ? { kind: "plaza", guild: params.get("guild")! }
        : { kind: "own" };
  return <House me={me} target={target} />;
}

/** `/house/:userId`: another player's house, unless they locked it. */
export function VisitHouse({ me }: { me: Me }) {
  const { userId = "" } = useParams();
  return <House me={me} target={{ kind: "visit", userId }} />;
}
function House({ me, target }: { me: Me; target: HouseTarget }) {
  const {
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
    guilds,
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
  } = useHousePage(target);
  return (
    <div className={playing ? "page house-playing" : "page"}>
      <TopBar me={me} />
      <main className="content content-wide">
        <div className="house-page">
          {!playing && (
            <Header
              title={title}
              house={house.data}
              targetKind={target.kind}
              visitId={visitId}
              updateVisitId={updateVisitId}
            />
          )}
          {!playing && house.data?.reef_loop && (
            <Suspense>
              <ReefLoop data={house.data.reef_loop} art={art.data} own={house.data.owner.you} />
            </Suspense>
          )}
          {house.isPending && <Loading label={t("Opening the door")} />}
          {house.isError && <Problem error={house.error} />}
          {house.isSuccess && (
            <>
              {!playing && (
                <Navigation
                  targetKind={target.kind}
                  playEnabled={playEnabled}
                  selectRoom={selectRoom}
                  selectScope={selectScope}
                  selectGuild={selectGuild}
                  toggleEdit={toggleEdit}
                  selectedGuild={selectedGuild}
                  guilds={guilds}
                  phone={phone}
                  editable={house.data.editable !== false}
                  canDecorate={canDecorate}
                  editing={editing}
                  rooms={house.data.rooms}
                  idx={idx}
                  selectMode={selectMode}
                />
              )}
              {playing && room.data && (
                <Suspense fallback={<Loading label={t("Opening Play")} />}>
                  <ReefStage
                    key={housePath(target)}
                    target={target}
                    editable={house.data.editable !== false && target.kind !== "visit"}
                    userId={me.user.id}
                    room={room.data}
                    own={house.data.scope === "own"}
                    roomKey={
                      target.kind === "plaza"
                        ? `plaza:${target.guild}:${room.data.idx}:${params.get("instance") ?? "0"}`
                        : `house:${house.data.owner.id}:${room.data.idx}`
                    }
                    onLayout={() => void room.refetch()}
                    roomName={summary?.name ?? house.data.house.name}
                    playerName={me.user.global_name ?? me.user.username}
                    onExit={() => selectMode("view")}
                    rooms={house.data.rooms}
                    onRoom={selectRoom}
                  />
                </Suspense>
              )}
              {!playing &&
                (room.isPending || art.isPending || (room.isSuccess && sprites.isPending)) &&
                !art.isError && <Loading label={t("Drawing the room")} />}
              {room.isError && <Problem error={room.error} />}
              {!playing && art.isError && <Problem error={art.error} />}
              {editing &&
                !playing &&
                house.data.editable !== false &&
                room.data &&
                art.data &&
                sprites.isSuccess && (
                  <Suspense fallback={<Loading label={t("Loading…")} />}>
                    <HouseEditor
                      key={housePath(target, `/rooms/${idx}`)}
                      target={target}
                      initial={room.data}
                      art={art.data}
                      house={house.data}
                      onExit={exitEdit}
                    />
                  </Suspense>
                )}
              {drawn && summary && (
                <>
                  <Suspense fallback={<Loading label={t("Drawing the room")} />}>
                    <RoomStage
                      rendered={drawn.rendered}
                      hover={hover}
                      onHover={hoverItem}
                      label={t("{name}, {count} items", {
                        name: roomLabel(summary.room_type, summary.name),
                        count: summary.items,
                      })}
                      botPicture={compare && bot.url ? bot.url : null}
                    />
                  </Suspense>
                  <div className="house-info" aria-live="polite">
                    {info ? (
                      <>
                        <span className="house-info-name">{info.label}</span>
                        {info.detail && <span className="muted">{info.detail}</span>}
                      </>
                    ) : (
                      <span className="muted">
                        {t(
                          phone
                            ? "Drag to explore. Use + and − to zoom."
                            : "Hover or tap a piece to see what it is. Drag to pan, Ctrl + wheel to zoom.",
                        )}
                      </span>
                    )}
                  </div>
                  <div className="house-options">
                    <PixelButton
                      variant={grid ? "primary" : "secondary"}
                      aria-pressed={grid}
                      disabled={compare}
                      onClick={toggleGrid}
                    >
                      {t("Tile grid")}
                    </PixelButton>
                    <PixelButton
                      variant={compare ? "primary" : "secondary"}
                      aria-pressed={compare}
                      onClick={toggleCompare}
                    >
                      {compare ? t("Back to room") : t("Compare with Discord")}
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
