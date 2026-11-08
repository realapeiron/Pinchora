import { t } from "../../i18n/reef-games";
import type { GameProps } from "./types";
export default function Trivia({ game, busy, send }: GameProps) {
  return (
    <section aria-label={t("Trivia round")}>
      <h3>{game.board.question}</h3>
      <div className="reef-trivia-options">
        {game.board.options?.map((option, choice) => (
          <button
            type="button"
            key={choice}
            disabled={busy || game.side === null || game.answered || game.board.phase !== "active"}
            onClick={() => send({ action: "answer", choice })}
          >
            {option}
            {game.board.correct === choice ? ` · ${t("Correct answer")}` : ""}
          </button>
        ))}
      </div>
      <p role="status">
        {game.board.phase === "done"
          ? t("Round finished")
          : game.side === null
            ? t("Spectating")
            : game.board.phase === "waiting"
              ? t("Waiting for players")
              : game.answered
                ? t("Answer locked. Waiting for the reveal.")
                : t("Choose one answer before the timer ends.")}
      </p>
      <ul>
        {game.players.map((p) => (
          <li key={p.id}>
            {p.id}: {t(p.answered ? "Answered" : game.board.phase === "done" ? "No answer" : "Thinking…")}
          </li>
        ))}
      </ul>
    </section>
  );
}
