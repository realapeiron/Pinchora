import Board from "./Board";
import type { GameProps } from "./types";
export default function Checkers(props: GameProps) {
  return <Board {...props} />;
}
