import { useState } from "react";
import { t } from "../../i18n/reef-games";
import type { GameProps } from "./types";
const pieces: Record<string, string> = {
  K: "♔",
  Q: "♕",
  R: "♖",
  B: "♗",
  N: "♘",
  P: "♙",
  k: "♚",
  q: "♛",
  r: "♜",
  b: "♝",
  n: "♞",
  p: "♟",
};
const names: Record<string, string> = {
  k: "King",
  q: "Queen",
  r: "Rook",
  b: "Bishop",
  n: "Knight",
  p: "Pawn",
};
export function chessCells(fen: string): string[] {
  return fen
    .split(" ")[0]!
    .split("/")
    .flatMap((row) => [...row].flatMap((c) => (/[1-8]/.test(c) ? Array<string>(Number(c)).fill(".") : [c])));
}
export default function Board({ game, busy, send }: GameProps) {
  const [selected, select] = useState<number | null>(null);
  const [promotion, promote] = useState("");
  const chess = game.kind === "chess";
  const cells = chess
    ? chessCells(game.board.fen ?? "8/8/8/8/8/8/8/8")
    : [...(game.board.cells ?? ".".repeat(32))];
  const enabled = !busy && game.board.phase === "active" && game.side === game.board.turn;
  const legal = game.board.legal ?? [];
  const moves = selected === null ? [] : legal.filter((m) => m.from === selected);
  const choose = (sq: number) => {
    const candidates = moves.filter((m) => m.to === sq);
    if (candidates.length === 1) {
      send({ action: "move", code: candidates[0]!.code });
      select(null);
    } else if (candidates.length > 1 && chess) promote(candidates[0]!.code.slice(0, 4));
    else select(sq);
  };
  return (
    <>
      <p>{t("Select a piece, then a legal destination. You can also choose a move from the list.")}</p>
      {game.board.check && <p role="status">{t("Check")}</p>}
      <div
        className="reef-board-scroll"
        tabIndex={0}
        role="region"
        aria-label={t(chess ? "Reef Chess" : "Pearl Checkers")}
      >
        <div className="reef-game-board" role="group" aria-label={t("Game board")}>
          <Squares
            chess={chess}
            cells={cells}
            selected={selected}
            moves={moves}
            enabled={enabled}
            choose={choose}
          />
        </div>
      </div>
      {promotion && (
        <fieldset>
          <legend>{t("Promote pawn")}</legend>
          {["q", "r", "b", "n"].map((p) => (
            <button
              key={p}
              type="button"
              disabled={!enabled || !legal.some((m) => m.code === promotion + p)}
              onClick={() => {
                send({ action: "move", code: promotion + p });
                promote("");
                select(null);
              }}
            >
              {t(names[p]!)}
            </button>
          ))}
        </fieldset>
      )}
      <label>
        {t("Legal moves")}
        <select
          aria-label={t("Legal moves")}
          disabled={!enabled}
          value=""
          onChange={(e) => {
            send({ action: "move", code: e.target.value });
            select(null);
          }}
        >
          <option value="">{t("Choose a move")}</option>
          {legal.map((m) => (
            <option value={m.code} key={m.code}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}

function Squares({
  chess,
  cells,
  selected,
  moves,
  enabled,
  choose,
}: {
  chess: boolean;
  cells: string[];
  selected: number | null;
  moves: NonNullable<GameProps["game"]["board"]["legal"]>;
  enabled: boolean;
  choose: (sq: number) => void;
}) {
  return (
    <>
      {Array.from({ length: 64 }, (_, display) => {
        const row = Math.floor(display / 8),
          col = display % 8;
        const dark = (row + col) % 2 === 1;
        const sq = chess ? (7 - row) * 8 + col : row * 4 + Math.floor(col / 2);
        const value = cells[chess ? display : sq] ?? ".";
        const coord = `${String.fromCharCode(97 + col)}${8 - row}`;
        if (!chess && !dark) return <span key={display} className="reef-game-square" />;
        const label =
          value === "."
            ? t("Empty")
            : chess
              ? `${t(value === value.toUpperCase() ? "White" : "Black")} ${t(names[value.toLowerCase()] ?? "Piece")}`
              : `${t(value.toLowerCase() === "a" ? "Pearl" : "Coral")} ${t(value === value.toUpperCase() ? "King" : "Piece")}`;
        const destination = moves.some((m) => m.to === sq);
        return (
          <button
            key={display}
            type="button"
            className={`reef-game-square ${dark ? "dark" : ""}`}
            data-square={sq}
            data-piece={value}
            data-destination={destination}
            aria-label={`${coord}: ${label}${destination ? ` · ${t("Legal destination")}` : ""}`}
            aria-pressed={selected === sq}
            aria-disabled={!enabled}
            onClick={() => {
              if (enabled) choose(sq);
            }}
          >
            <span aria-hidden="true">
              {value === "."
                ? destination
                  ? "·"
                  : ""
                : chess
                  ? pieces[value]
                  : value === value.toUpperCase()
                    ? value === "A"
                      ? "◎"
                      : "◉"
                    : value === "a"
                      ? "○"
                      : "●"}
            </span>
          </button>
        );
      })}
    </>
  );
}
