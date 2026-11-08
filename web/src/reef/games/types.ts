export type LegalMove = { code: string; label: string; from: number; to: number };
export type Game = {
  id: string;
  room: string;
  kind: "checkers" | "chess" | "trivia";
  revision: string;
  host_id: string;
  deadline: string;
  practice: boolean;
  side: number | null;
  answered: boolean;
  placement_id: string | null;
  players: { id: string; side: number; paid: number; answered: boolean }[];
  board: {
    phase: string;
    turn: number | null;
    winner: number | null;
    reason: string | null;
    cells?: string;
    fen?: string;
    legal?: LegalMove[];
    check?: boolean;
    question?: string;
    options?: string[];
    correct?: number;
  };
};
export type Intent =
  | { action: "join" | "leave" | "start" }
  | { action: "move"; code: string }
  | { action: "answer"; choice: number };
export type GameProps = { game: Game; busy: boolean; send: (intent: Intent) => void };

export const names = { checkers: "Pearl Checkers", chess: "Reef Chess", trivia: "Trivia" };
