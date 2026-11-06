// The pictures the bot drew for the site (`write_landing_assets` in pinchora-commands), with their sizes so
// the page can hold their place before they load.

import badges from "./showcase/badges.png";
import chess from "./showcase/chess.png";
import connect4 from "./showcase/connect4.png";
import leaderboard from "./showcase/leaderboard.png";
import levelup from "./showcase/levelup.png";
import pet from "./showcase/pet.png";
import profile from "./showcase/profile.png";
import rank from "./showcase/rank.png";
import room from "./showcase/room.png";
import wallet from "./showcase/wallet.png";
import welcome from "./showcase/welcome.png";
import wordle from "./showcase/wordle.png";
import { FEATURES, type ShotKey } from "./features";

export interface Shot {
  src: string;
  width: number;
  height: number;
}

export const SHOTS: Record<ShotKey, Shot> = {
  profile: { src: profile, width: 960, height: 540 },
  room: { src: room, width: 636, height: 456 },
  chess: { src: chess, width: 580, height: 730 },
  connect4: { src: connect4, width: 580, height: 670 },
  wordle: { src: wordle, width: 440, height: 660 },
  welcome: { src: welcome, width: 1100, height: 500 },
  rank: { src: rank, width: 934, height: 282 },
  wallet: { src: wallet, width: 900, height: 300 },
  pet: { src: pet, width: 860, height: 420 },
  leaderboard: { src: leaderboard, width: 900, height: 716 },
  badges: { src: badges, width: 960, height: 826 },
  levelup: { src: levelup, width: 720, height: 220 },
};

/** What a picture shows, for people who cannot see it. */
export function altOf(key: ShotKey): string {
  for (const feature of FEATURES) {
    const shot = feature.shots.find((s) => s.key === key);
    if (shot) return shot.alt;
  }
  return "";
}
