import Board from "./Board";
import type { GameProps } from "./types";
export default function Chess(props: GameProps) {
  return <Board {...props} />;
}
