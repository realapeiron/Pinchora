import { lazy } from "react";
import { t } from "../../i18n/reef-games";
import { names, type GameProps } from "./types";
const Checkers = lazy(() => import("./Checkers"));
const Chess = lazy(() => import("./Chess"));
const Trivia = lazy(() => import("./Trivia"));
export default function Match({ game, busy, send, user, now }: GameProps & { user: string; now: number }) {
  return (
    <section data-game-id={game.id} data-phase={game.board.phase}>
      <h3>{t(names[game.kind])}</h3>
      <p role="status">
        {game.board.phase === "done"
          ? game.board.winner === null
            ? t("Round finished")
            : t("Player {count} wins", { count: game.board.winner + 1 })
          : game.side === null
            ? t("Spectating")
            : game.board.phase === "waiting"
              ? t("Waiting for players")
              : game.kind === "trivia"
                ? t("Playing")
                : game.board.turn === game.side
                  ? t("Your turn")
                  : t("Waiting for the other player")}
      </p>
      {game.board.phase !== "done" && (
        <p>
          {t("Time remaining: {count}s", {
            count: Math.max(0, Math.ceil((Date.parse(game.deadline) - now) / 1000)),
          })}
        </p>
      )}
      {game.board.phase === "waiting" && game.side === null && (
        <button type="button" disabled={busy} onClick={() => send({ action: "join" })}>
          {t("Join game")}
        </button>
      )}
      {game.kind === "trivia" && game.board.phase === "waiting" && game.host_id === user && (
        <button
          type="button"
          disabled={busy || game.players.length < 2}
          onClick={() => send({ action: "start" })}
        >
          {t("Start round")}
        </button>
      )}
      {game.side !== null && game.board.phase !== "done" && (
        <button type="button" disabled={busy} onClick={() => send({ action: "leave" })}>
          {t("Leave game")}
        </button>
      )}
      {game.kind === "checkers" ? (
        <Checkers key={game.id} game={game} busy={busy} send={send} />
      ) : game.kind === "chess" ? (
        <Chess key={game.id} game={game} busy={busy} send={send} />
      ) : (
        <Trivia game={game} busy={busy} send={send} />
      )}
      {game.board.phase === "done" && (
        <p>{t("Shells earned: {count}", { count: game.players.find((p) => p.id === user)?.paid ?? 0 })}</p>
      )}
    </section>
  );
}
